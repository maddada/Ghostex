//! Reminders on drift: the one-line reminder gxserver adds under the next message it delivers to a
//! coordinator whose last reply drifted from its playbook, and the note that sends a coordinator
//! back to its guide after a compaction its hooks could not report.
//!
//! CDXC:Coordinators 2026-11-13 DECISION:
//! User (Q2980 "Build it: auto-send + remind on drift"): when a coordinator's reply is missing the marked sections (while Marked handovers asks coordinators for them) or asks the user a question in plain text instead of an item ask, a form or a page, Ghostex adds a one-line reminder to the next message it delivers to that coordinator, at most once per `DRIFT_REMINDER_TURNS` turns so it doesn't nag; after a compaction it tells the coordinator to re-run `ghostex coordinator guide`. Then, through the coordinator: "the drift is mostly (1) the handover missing the correct marked sections, (2) questions asked in plain text instead of as a form (```questions block / item ask) or a page (inline HTML), and (3) rarer: the coordinator starting the work itself instead of starting threads", with a reminder that names each slip in one line ("Reminder: ask that as a form or an item ask, not in plain text; hand real work to a thread with start-thread; end with the marked sections.").
//! Detection is cheap and conservative, because a false reminder is worse than a missed one: the reply is judged once, when the next message is delivered (no hook or turn-end work), and only while the coordinator is not mid-turn; an unmarked reply counts only when it is longer than a short acknowledgement and holds no marker emoji at all; a question counts only when the reply's last paragraph ends with "?", outside code, with no form (```questions) or page in the reply, no ask tag named in it, and no item ask made since the coordinator's turn started; working itself counts only when the turn (from the prompt that started it, inside the transcript tail read) edited a file of its own project outside `docs/`, or ran a build or test command (cargo build/test, bun run, npm, xcodebuild and the like; never a `ghostex` call or `cargo xtask rebuild`, which coordinators run), and ran no `start-thread`. A turn is a new reply judged here. The compaction note is for the agents whose hooks cannot hand a coordinator its role again (Claude and Codex get it as SessionStart context, role.rs `coordinator_role_again`), read from the "Context compacted" row their transcript decoders write; the first look at a coordinator only notes the compaction it already had.
//! SEE-ALSO: server/src/session_chat_queue_runtime/send.rs (adds the notes and records them once delivered), packages/gx-chat-core/src/session/appended_notes.rs (the note texts every chat hides), server/src/coordinators/playbook.rs (`playbook_update_note`, the other note on the same seam).

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use super::records::{now_iso, SessionKey};
use crate::session_chat::{SessionChatBlock, SessionChatMessage, SessionChatRole};
use ghostex_gx_chat_core::session::appended_notes::{
    drift_reminder, DriftSlip, CONTEXT_COMPACTED_NOTE,
};

/// At most one drift reminder per this many of the coordinator's turns.
pub const DRIFT_REMINDER_TURNS: u64 = 5;

/// A reply shorter than this may go without its marked sections (an acknowledgement).
const MIN_UNMARKED_REPLY_CHARS: usize = 200;

/// Messages read from the transcript tail to find the newest reply and compaction.
const DRIFT_SCAN_LIMIT: usize = 60;

fn metadata_key(key: &SessionKey) -> String {
    format!("coordinatorDrift:{}:{}", key.0, key.1)
}

/// What gxserver remembers of a coordinator between deliveries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DriftRecord {
    /// The newest reply already judged (`<message id>:<length>`).
    judged: String,
    /// Replies judged so far: the coordinator's turns as this check counts them.
    turns: u64,
    /// The turn the last reminder went with.
    reminded_at_turn: Option<u64>,
    /// The newest compaction already handled.
    compaction: Option<String>,
}

impl DriftRecord {
    fn from_value(value: &Value) -> Self {
        Self {
            judged: value["judged"].as_str().unwrap_or_default().to_string(),
            turns: value["turns"].as_u64().unwrap_or_default(),
            reminded_at_turn: value["remindedAtTurn"].as_u64(),
            compaction: value["compaction"].as_str().map(str::to_string),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "judged": self.judged,
            "turns": self.turns,
            "remindedAtTurn": self.reminded_at_turn,
            "compaction": self.compaction,
        })
    }
}

/// The notes for the message on its way to a coordinator, and the record to keep once it is
/// delivered ([`record_coordinator_drift`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriftDelivery {
    pub notes: Vec<String>,
    record: DriftRecord,
    /// The record changed (a reply was judged or a compaction seen), so it is written.
    changed: bool,
}

fn read_record(db: &Connection, key: &SessionKey) -> Option<DriftRecord> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [metadata_key(key)],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    .map(|value| DriftRecord::from_value(&value))
}

/// Keeps what a delivered message's notes were based on, so the same reply is not judged again
/// and the next reminder waits its turns.
pub fn record_coordinator_drift(db: &Connection, key: &SessionKey, delivery: &DriftDelivery) {
    if !delivery.changed {
        return;
    }
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![
            metadata_key(key),
            delivery.record.to_value().to_string(),
            now_iso()
        ],
    );
}

fn text_of(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.trim()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The transcript tail of a session, newest last; `None` when it cannot be read.
fn transcript_tail(session: &Value) -> Option<Vec<SessionChatMessage>> {
    use crate::presentation::read_runtime_text;
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(
        crate::session_chat_follower::session_chat_agent_for_session(session).as_deref(),
    )?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    match crate::session_chat::read_session_chat_tail_page(agent, &path, DRIFT_SCAN_LIMIT, None)
        .ok()?
    {
        crate::session_chat::SessionChatTailPage::Page { messages, .. } => Some(messages),
        crate::session_chat::SessionChatTailPage::NotFound => None,
    }
}

/// The newest reply as `(identity, markdown)`, only while it is the transcript's newest text (a
/// prompt after it means the next turn's reply is not written yet).
fn newest_reply(messages: &[SessionChatMessage]) -> Option<(String, String)> {
    let (message, text) = messages
        .iter()
        .rev()
        .filter(|message| !message.queued && message.role != SessionChatRole::System)
        .map(|message| (message, text_of(message)))
        .find(|(_, text)| !text.is_empty())?;
    (message.role == SessionChatRole::Assistant)
        .then(|| (format!("{}:{}", message.id, text.len()), text))
}

/// The newest "Context compacted" row's id.
fn newest_compaction(messages: &[SessionChatMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|message| {
            message.role == SessionChatRole::System
                && text_of(message) == crate::session_chat::CONTEXT_COMPACTED_STATUS_TEXT
        })
        .map(|message| message.id.clone())
}

/// `markdown` without its fenced code blocks.
fn without_code(markdown: &str) -> String {
    let mut out = Vec::new();
    let mut fenced = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            out.push(line);
        }
    }
    out.join("\n")
}

/// Whether `text` names an ask tag (`Q12`) or a work item (`W7`), as a reply pointing at an ask in
/// the dock does.
fn names_ask_tag(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            let mut chars = word.chars();
            matches!(chars.next(), Some('Q' | 'W'))
                && word.len() > 1
                && chars.all(|c| c.is_ascii_digit())
        })
}

/// The reply ends on a question to the user written in its text, not a form or a page.
fn asks_in_text(reply: &str) -> bool {
    let lower = reply.to_ascii_lowercase();
    if lower.contains("```questions") || lower.contains("```html") || lower.contains("<html") {
        return false;
    }
    let prose = without_code(reply);
    let Some(last) = prose
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .last()
    else {
        return false;
    };
    let Some(line) = last.lines().map(str::trim).rfind(|line| !line.is_empty()) else {
        return false;
    };
    if line.starts_with('>') {
        return false;
    }
    line.trim_end_matches(['*', '_', '`', ' ']).ends_with('?') && !names_ask_tag(last)
}

/// The reply goes without the marked sections it owes.
fn misses_marked_sections(reply: &str) -> bool {
    reply.chars().count() >= MIN_UNMARKED_REPLY_CHARS
        && !reply.contains("```questions")
        && !ghostex_gx_chat_core::transcript::marked_sections::has_any_marker(reply)
}

/// The tool calls of the newest turn: those after the last prompt in `messages`. `None` when the
/// tail read holds no prompt (a turn longer than the tail), so a part of a turn is never judged.
fn turn_tool_calls(messages: &[SessionChatMessage]) -> Option<Vec<(&str, &Value)>> {
    let prompt = messages.iter().rposition(|message| {
        message.role == SessionChatRole::User
            && message.blocks.iter().any(
                |block| matches!(block, SessionChatBlock::Text { text } if !text.trim().is_empty()),
            )
    })?;
    Some(
        messages[prompt + 1..]
            .iter()
            .flat_map(|message| message.blocks.iter())
            .filter_map(|block| match block {
                SessionChatBlock::ToolCall { name, input, .. } => Some((name.as_str(), input)),
                _ => None,
            })
            .collect(),
    )
}

/// The shell command a tool call runs, for the agents' shell tools (Claude's `Bash`, Codex's
/// `exec_command` and `shell`, and the like).
fn shell_command(name: &str, input: &Value) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    if !(lower.contains("bash")
        || lower.contains("shell")
        || lower.contains("exec")
        || lower.contains("terminal")
        || lower == "run_command")
    {
        return None;
    }
    let command = input
        .get("command")
        .or_else(|| input.get("cmd"))
        .unwrap_or(input);
    match command {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// The files a tool call edits: Claude's `Edit`, `MultiEdit`, `Write` and `NotebookEdit`, and
/// the files an `apply_patch` names.
fn edited_files(name: &str, input: &Value) -> Vec<String> {
    let lower = name.to_ascii_lowercase();
    if lower == "apply_patch" || lower.ends_with("apply_patch") {
        let patch = input
            .get("input")
            .or_else(|| input.get("patch"))
            .and_then(Value::as_str)
            .or_else(|| input.as_str())
            .unwrap_or_default();
        return patch
            .lines()
            .filter_map(|line| {
                ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                    .iter()
                    .find_map(|prefix| line.strip_prefix(prefix))
            })
            .map(|path| path.trim().to_string())
            .collect();
    }
    if !matches!(
        lower.as_str(),
        "edit"
            | "multiedit"
            | "write"
            | "notebookedit"
            | "str_replace_editor"
            | "write_file"
            | "edit_file"
    ) {
        return Vec::new();
    }
    ["file_path", "path", "notebook_path", "filePath"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .map(|path| vec![path.to_string()])
        .unwrap_or_default()
}

/// A file of the coordinator's own project (its `cwd`) outside `docs/`: real work, not notes.
fn is_project_file(path: &str, cwd: Option<&str>) -> bool {
    let path = path.trim();
    let relative = if path.starts_with('/') || path.starts_with('~') {
        let Some(cwd) = cwd
            .map(|cwd| cwd.trim_end_matches('/'))
            .filter(|cwd| !cwd.is_empty())
        else {
            return false;
        };
        match path
            .strip_prefix(cwd)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            Some(rest) => rest,
            None => return false,
        }
    } else {
        path.trim_start_matches("./")
    };
    !relative.is_empty()
        && !relative.starts_with("docs/")
        && !relative.starts_with("..")
        && !relative.contains("agent-work")
}

/// A shell command that builds or tests (cargo build/test, bun run, npm, xcodebuild and the like).
/// A `ghostex` call and `cargo xtask` (coordinators run `cargo xtask rebuild`) never count.
fn builds_or_tests(command: &str) -> bool {
    const BUILDS: [&str; 16] = [
        "cargo build",
        "cargo test",
        "cargo check",
        "cargo clippy",
        "cargo run",
        "bun run",
        "bun test",
        "npm ",
        "pnpm ",
        "yarn ",
        "xcodebuild",
        "swift build",
        "swift test",
        "gradlew",
        "go build",
        "go test",
    ];
    command.split(['\n', ';', '|', '&']).any(|part| {
        let part = part.trim();
        !part.starts_with("ghostex")
            && !part.contains("cargo xtask")
            && BUILDS
                .iter()
                .any(|build| part.starts_with(build) || part.contains(&format!(" {build}")))
    })
}

/// The newest turn did the work itself: it edited its project's files or ran a build or test, and
/// started no thread.
fn worked_itself(messages: &[SessionChatMessage], session: &Value) -> bool {
    let Some(calls) = turn_tool_calls(messages) else {
        return false;
    };
    let cwd = session.get("cwd").and_then(Value::as_str);
    let mut worked = false;
    for (name, input) in calls {
        if let Some(command) = shell_command(name, input) {
            if command.contains("start-thread") {
                return false;
            }
            worked |= builds_or_tests(&command);
        }
        worked |= edited_files(name, input)
            .iter()
            .any(|path| is_project_file(path, cwd));
    }
    worked
}

/// The coordinator made an item ask in its current turn (since its last recorded turn start), or,
/// with no turn start known, has any ask open.
fn asked_this_turn(db: &Connection, key: &SessionKey) -> bool {
    let start = super::turns::read_turn_starts(db)
        .remove(&format!("{}:{}", key.0, key.1))
        .and_then(|(_, current)| chrono::DateTime::parse_from_rfc3339(&current).ok());
    let Ok(items) = super::work::list_work_items(db, key) else {
        return true;
    };
    items
        .iter()
        .flat_map(|item| item.asks.iter())
        .any(|ask| match start {
            Some(start) => chrono::DateTime::parse_from_rfc3339(&ask.asked_at)
                .map_or(true, |asked| asked >= start),
            None => ask.is_open(),
        })
}

/// The drift reminder and compaction note for the next message to the coordinator `key` (its
/// session row `session`), with the record to keep once it is delivered; `None` for a session that
/// is not a coordinator.
pub fn coordinator_drift_delivery(
    db: &Connection,
    paths: &crate::paths::GxserverPaths,
    key: &SessionKey,
    session: &Value,
) -> Option<DriftDelivery> {
    super::records::read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()?;
    let stored = read_record(db, key);
    let first_look = stored.is_none();
    let mut record = stored.unwrap_or_default();
    let mut notes = Vec::new();
    let mut changed = first_look;
    let messages = transcript_tail(session).unwrap_or_default();

    // Claude and Codex coordinators get their role back from their SessionStart hook.
    let hook_hands_role_back = matches!(
        crate::session_chat_follower::session_chat_agent_for_session(session).as_deref(),
        Some("claude" | "codex")
    );
    if let Some(compaction) = newest_compaction(&messages) {
        if record.compaction.as_deref() != Some(compaction.as_str()) {
            if !first_look && !hook_hands_role_back {
                notes.push(CONTEXT_COMPACTED_NOTE.to_string());
            }
            record.compaction = Some(compaction);
            changed = true;
        }
    }

    let mid_turn = crate::session_chat_follower::session_chat_hook_working(session);
    if let Some((identity, reply)) = newest_reply(&messages).filter(|_| !mid_turn) {
        if identity != record.judged {
            record.judged = identity;
            record.turns += 1;
            changed = true;
            let due = record
                .reminded_at_turn
                .is_none_or(|at| record.turns >= at + DRIFT_REMINDER_TURNS);
            if due {
                let mut slips = Vec::new();
                if asks_in_text(&reply) && !asked_this_turn(db, key) {
                    slips.push(DriftSlip::AskedInText);
                }
                if worked_itself(&messages, session) {
                    slips.push(DriftSlip::WorkedItself);
                }
                if crate::agents::coordinator_marks_replies(paths) && misses_marked_sections(&reply)
                {
                    slips.push(DriftSlip::Unmarked);
                }
                if let Some(reminder) = drift_reminder(&slips) {
                    notes.push(reminder);
                    record.reminded_at_turn = Some(record.turns);
                }
            }
        }
    }
    Some(DriftDelivery {
        notes,
        record,
        changed,
    })
}
