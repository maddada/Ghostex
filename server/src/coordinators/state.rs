//! What a thread is doing, decided once in gxserver for the supervisor, the sidebar, the chat's
//! Threads panel and `ghostex coordinator status`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::records::SessionKey;

use crate::agents::session_chat_prompt_setting;
use crate::presentation::{effective_lifecycle_state, presentation_activity};
use crate::session_chat_interactive::{
    parse_stored_session_chat_prompt, SessionChatInteractivePrompt,
};
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SESSION_CHAT_NOTICE_AGENT_EXITED,
    SESSION_CHAT_NOTICE_PERMISSION_PROMPT, SESSION_CHAT_NOTICE_USAGE_LIMIT,
};
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;

/// CDXC:Coordinators 2026-09-30 WHY:
/// Claude's Overview groups threads as Ready for review, Waiting on you, Working, Landing, Idle and Resolved. The first and fourth depend on pull requests, which a Ghostex thread does not always open, so a finished turn is one state (`finished`) and the report says whether there is a branch or pull request. A sleeping thread is still open work, not done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// A question, an approval, or another prompt that needs an answer is on its screen.
    Waiting,
    Working,
    /// Its turn ended and nobody resolved it yet.
    Finished,
    Sleeping,
    /// Its session was closed or no longer exists.
    Closed,
    /// Resolved by the user or the coordinator.
    Done,
}

impl ThreadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Working => "working",
            Self::Finished => "finished",
            Self::Sleeping => "sleeping",
            Self::Closed => "closed",
            Self::Done => "done",
        }
    }

    /// The heading the CLI prints over a group of threads in this state.
    pub fn heading(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting on you",
            Self::Working => "Working",
            Self::Finished => "Finished",
            Self::Sleeping => "Sleeping",
            Self::Closed => "Closed",
            Self::Done => "Done",
        }
    }

    /// Display order: what needs someone first.
    pub fn order(self) -> u8 {
        match self {
            Self::Waiting => 0,
            Self::Finished => 1,
            Self::Working => 2,
            Self::Sleeping => 3,
            Self::Closed => 4,
            Self::Done => 5,
        }
    }
}

/// The question or approval a thread is showing, as one line for a report, and a key that changes
/// whenever a different prompt appears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadPrompt {
    pub key: String,
    pub summary: String,
}

/// What the thread record adds to the session's own state.
#[derive(Clone, Copy, Debug)]
pub struct ThreadProgress {
    pub resolved: bool,
    /// It has run a turn since it was started (seen working, or a turn was already reported).
    pub has_run: bool,
}

impl ThreadProgress {
    pub fn of(thread: &super::records::ThreadRecord) -> Self {
        Self {
            resolved: thread.is_resolved(),
            has_run: thread.observed_working || thread.reported_at.is_some(),
        }
    }
}

/// `transcript_working` is the supervisor's own reading of the transcript (mid-turn or not); the
/// projection passes false and relies on the hook state alone.
pub fn classify_thread_session(
    session: Option<&Value>,
    progress: ThreadProgress,
    now_iso: &str,
    transcript_working: bool,
) -> ThreadState {
    if progress.resolved {
        return ThreadState::Done;
    }
    let Some(session) = session else {
        return ThreadState::Closed;
    };
    match effective_lifecycle_state(session).as_str() {
        "running" => {}
        "sleeping" => return ThreadState::Sleeping,
        _ => return ThreadState::Closed,
    }
    // CDXC:Coordinators 2026-10-15 DECISION: User: "report to the orchestrator once the fast stage gives up". A thread whose turn ended on a connection or server error is still working while one of Ghostex's quick tries is ahead; it is reported, with the slow schedule noted, only after they all failed (accounts/api_errors.rs).
    if crate::accounts::api_errors::quick_retry_pending(session) {
        return ThreadState::Working;
    }
    let activity = presentation_activity(session, now_iso);
    let working = activity == "working" || transcript_working;
    if applicable_screen_wait(session, working).is_some() {
        return ThreadState::Waiting;
    }
    if working {
        return ThreadState::Working;
    }
    if thread_prompt(session).is_some() {
        return ThreadState::Waiting;
    }
    // A Stop hook's attention is a finished turn; any other attention (a permission, a bell, an
    // "Action Required" title) is the agent waiting for someone.
    if activity == "attention"
        && attention_source(session).as_deref() != Some(TURN_COMPLETE_ATTENTION_SOURCE)
    {
        return ThreadState::Waiting;
    }
    // A thread whose brief has not run yet is starting, not finished: its agent is still coming
    // up, or its first message is waiting for the input box.
    if !progress.has_run {
        return ThreadState::Working;
    }
    ThreadState::Finished
}

fn attention_source(session: &Value) -> Option<String> {
    session
        .pointer("/runtimeSettings/agentActivity/attentionSource")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// A waiting thread is blocked on something only the user can grant: an approval card, or attention
/// that is no question at all (a permission notification, a folder-trust dialog). A question in its
/// chat is the coordinator's to answer, so it does not count.
/// A screen wait (Empryo's only signal) follows the same rule: a screen that blocks input is the
/// user's, a question read off the screen is the coordinator's.
pub fn thread_needs_user_approval(session: &Value) -> bool {
    let screen = recorded_screen_wait(session);
    if screen.as_ref().is_some_and(|wait| wait.blocking) {
        return true;
    }
    if let Some(prompt) = session_chat_prompt_setting(session)
        .and_then(|stored| parse_stored_session_chat_prompt(&stored))
    {
        return matches!(prompt, SessionChatInteractivePrompt::Approval { .. });
    }
    if screen.is_some() {
        return false;
    }
    crate::session_chat_async_questions::pending_question_count(session) == 0
}

/// The prompt a thread is showing: the stored question or approval card, or the async questions
/// Claude asks without blocking.
pub fn thread_prompt(session: &Value) -> Option<ThreadPrompt> {
    if let Some(stored) = session_chat_prompt_setting(session) {
        if let Some(prompt) = parse_stored_session_chat_prompt(&stored) {
            let summary = prompt_summary(&prompt);
            let key = match &prompt {
                SessionChatInteractivePrompt::Approval { .. } => {
                    crate::session_approval::approval_wait_id(session)
                        .map(str::to_owned)
                        .unwrap_or_else(|| stored_prompt_key(&stored))
                }
                SessionChatInteractivePrompt::Question { .. } => stored_prompt_key(&stored),
            };
            return Some(ThreadPrompt { key, summary });
        }
    }
    let pending = crate::session_chat_async_questions::pending_question_count(session);
    if pending > 0 {
        let ids = session
            .pointer("/runtimeSettings/sessionChatAsyncQuestionIds")
            .map(Value::to_string)
            .unwrap_or_default();
        return Some(ThreadPrompt {
            key: stored_prompt_key(&ids),
            summary: if pending == 1 {
                "It asked a question in its chat.".to_string()
            } else {
                format!("It asked {pending} questions in its chat.")
            },
        });
    }
    None
}

/// One line per question (with its options), or the approval and what it is for.
fn prompt_summary(prompt: &SessionChatInteractivePrompt) -> String {
    match prompt {
        SessionChatInteractivePrompt::Question { questions, .. } => questions
            .iter()
            .map(|question| {
                let options = question
                    .options
                    .iter()
                    .map(|option| option.label.trim())
                    .filter(|label| !label.is_empty())
                    .collect::<Vec<_>>();
                if options.is_empty() {
                    question.question.trim().to_string()
                } else {
                    format!(
                        "{} (options: {})",
                        question.question.trim(),
                        options.join(" / ")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        SessionChatInteractivePrompt::Approval { tool, summary, .. } => match summary {
            Some(summary) if !summary.trim().is_empty() => {
                format!("Approval for {tool}: {}", summary.trim())
            }
            _ => format!("Approval for {tool}"),
        },
    }
}

/// What the chat last read off a thread's screen that only a person can answer.
#[derive(Clone, Debug)]
pub struct ThreadScreenWait {
    prompt: ThreadPrompt,
    /// A screen that blocks input (folder trust, a login, a usage limit, an approval) rather than
    /// a question read off it.
    blocking: bool,
    /// A question or an approval, which counts even while hooks say working.
    asks: bool,
    /// A permission prompt or an approval read off the screen: the user's to grant.
    approval: bool,
}

impl ThreadScreenWait {
    /// What the chat's cached reading of a thread's screen waits on: a screen that blocks input
    /// first (one Ghostex is answering on its own does not count), then a question read off it.
    pub fn from_screen(
        notice: Option<&SessionChatTerminalNotice>,
        prompt: Option<&SessionChatInteractivePrompt>,
        thread_ref: &str,
    ) -> Option<Self> {
        if let Some(notice) = notice.filter(|notice| notice.blocks_input() && !notice.auto_trust) {
            let title = notice.title.trim();
            let detail = notice
                .detail
                .as_deref()
                .map(str::trim)
                .filter(|detail| !detail.is_empty())
                .map(|detail| format!("{detail}\n\n"))
                .unwrap_or_default();
            let next = screen_fix(notice, thread_ref)
                .unwrap_or_else(|| "Someone has to answer it in that thread.".to_string());
            let summary = format!("Its screen shows: {title}\n\n{detail}{next}");
            return Some(Self {
                prompt: ThreadPrompt {
                    key: format!("notice:{}:{}", notice.kind, notice.title),
                    summary,
                },
                blocking: true,
                asks: notice.kind == SESSION_CHAT_NOTICE_PERMISSION_PROMPT,
                approval: notice.kind == SESSION_CHAT_NOTICE_PERMISSION_PROMPT,
            });
        }
        let prompt = prompt?;
        let summary = prompt_summary(prompt);
        Some(Self {
            prompt: ThreadPrompt {
                key: format!("screen:{}", stored_prompt_key(&summary)),
                summary,
            },
            blocking: false,
            asks: true,
            approval: matches!(prompt, SessionChatInteractivePrompt::Approval { .. }),
        })
    }
}

/// The `ghostex` verbs that fix a screen an orchestrator can fix itself: a start that failed on an
/// account without a saved login, a usage limit, an agent that exited.
fn screen_fix(notice: &SessionChatTerminalNotice, thread_ref: &str) -> Option<String> {
    let switch = format!(
        "run `ghostex accounts`, then `ghostex agents switch-account {thread_ref} --account <number>` with a ready account that has room; it resumes the same conversation there"
    );
    if notice.missing_login() {
        let slot = notice
            .missing_login_slot()
            .map(|slot| format!(" {slot}"))
            .unwrap_or_default();
        return Some(format!(
            "To fix it, {switch}. If no account works, ask the user to sign in to account{slot} (Settings > Accounts)."
        ));
    }
    if notice.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT {
        return Some(format!("To continue before the reset, {switch}."));
    }
    if notice.kind == SESSION_CHAT_NOTICE_AGENT_EXITED {
        return Some(format!(
            "`ghostex agents reload {thread_ref}` starts it again on the same conversation."
        ));
    }
    None
}

/// CDXC:Coordinators 2026-10-06 WHY:
/// Empryo has no hook for a question or an approval: the PreToolUse of the tool that asks leaves its hooks at working while its choice panel waits, so its thread never showed as waiting. The supervisor records what the chat's cached screen reading shows for every running thread each tick, and every surface (its reports, the Threads panel, `ghostex coordinator status`, the sidebar's crew count) classifies from that record. While hooks say working only a question or a permission prompt counts (Claude's own permission prompt moves its hooks to attention, so Claude and Codex threads read as before); any other blocking screen counts once hooks stop saying working. The record is rebuilt from the cache every tick, and the cache is re-read when the screen changes and after an answer, so an answered question leaves with the next tick.
/// SEE-ALSO: server/src/server/coordinator_runtime.rs (`refresh_thread_screen_waits`), server/src/session_chat_empryo_question.rs (the panel reading).
fn thread_screen_waits() -> &'static Mutex<HashMap<SessionKey, ThreadScreenWait>> {
    static WAITS: OnceLock<Mutex<HashMap<SessionKey, ThreadScreenWait>>> = OnceLock::new();
    WAITS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Replaces every thread's screen wait with the supervisor's latest reading.
pub fn replace_thread_screen_waits(waits: HashMap<SessionKey, ThreadScreenWait>) {
    if let Ok(mut current) = thread_screen_waits().lock() {
        *current = waits;
    }
}

/// The thread's last screen reading. An approval read off the screen of a session whose hooks
/// report its approvals is left to its card (`session_approval::hooks_report_approvals`).
fn recorded_screen_wait(session: &Value) -> Option<ThreadScreenWait> {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let wait = thread_screen_waits()
        .lock()
        .ok()?
        .get(&(text("projectId"), text("sessionId")))
        .cloned()?;
    (!wait.approval || !crate::session_approval::hooks_report_approvals(session)).then_some(wait)
}

fn applicable_screen_wait(session: &Value, hook_working: bool) -> Option<ThreadScreenWait> {
    recorded_screen_wait(session).filter(|wait| wait.asks || !hook_working)
}

/// True when a thread's last screen reading shows a permission prompt or an approval, which only
/// the user can grant (Empryo, Copilot and the other agents whose approvals exist only on screen).
pub fn screen_waits_on_approval(session: &Value) -> bool {
    recorded_screen_wait(session).is_some_and(|wait| wait.approval)
}

/// True when the thread waits on something its screen shows; the supervisor's transcript check
/// does not overrule that, since the turn that asked is still open.
pub fn waits_on_screen(session: &Value) -> bool {
    recorded_screen_wait(session).is_some()
}

/// What a waiting thread waits on: a screen that blocks input, else its stored question or
/// approval card, else a question read off its screen. An approval card is keyed by its
/// PermissionRequest (`session_approval::approval_wait_id`), so the same command asked again is
/// a new report.
/// Legacy rows without approval metadata keep their stored-card keys; a first late hook can
/// still follow a delivered screen report with its new UUID key.
pub fn waiting_prompt(session: &Value) -> Option<ThreadPrompt> {
    let screen = recorded_screen_wait(session);
    match screen {
        Some(wait) if wait.blocking => Some(wait.prompt),
        _ => thread_prompt(session)
            .map(|prompt| ThreadPrompt {
                key: format!("prompt:{}", prompt.key),
                summary: prompt.summary,
            })
            .or(screen.map(|wait| wait.prompt)),
    }
}

fn stored_prompt_key(stored: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(stored.as_bytes()))
        .chars()
        .take(16)
        .collect()
}

/// CDXC:Coordinators 2026-09-30 DECISION:
/// User (question 1, answer 1B): a coordinator's threads trust the worktrees of a project the user already trusted, so a worktree thread starts without stopping at the agent's folder-trust question. The project counts as trusted because its coordinator runs in it (the user created it there and answered that folder's own question). Only an open thread qualifies, and only in the project's own folder or a worktree Ghostex made for it; any other folder still asks.
/// SEE-ALSO: server/src/session_chat_trust_memory.rs (`session_folders_remembered` asks this), the "Trust and Remember" decision there.
pub fn coordinator_thread_folder_trusted(
    repository: &crate::domain::DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> bool {
    let Ok(Some(thread)) = super::records::read_thread(repository.db, project_id, session_id)
    else {
        return false;
    };
    if thread.is_resolved() {
        return false;
    }
    let coordinator_alive = repository
        .get_session(
            &thread.coordinator_project_id,
            &thread.coordinator_session_id,
        )
        .ok()
        .flatten()
        .is_some();
    if !coordinator_alive || thread.coordinator_project_id != project_id {
        return false;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let cwd = session.get("cwd").and_then(Value::as_str).map(str::trim);
    let project_path = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| {
            project
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    match cwd.filter(|cwd| !cwd.is_empty()) {
        None => true,
        Some(cwd) if project_path.as_deref().map(str::trim) == Some(cwd) => true,
        Some(cwd) => crate::worktree_sessions::read_worktree_session_marker(&session)
            .is_some_and(|marker| marker.path.trim() == cwd),
    }
}
