//! The to-dos themselves, kept in the session's own record (`runtimeSettings.sessionNeedsYou`), so
//! they outlive a cleared chat, a restart and a sleeping session, and every presentation of the
//! session can count them without reading anything else.

use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::domain::DomainStateError;

/// The key in `runtimeSettings`.
pub(crate) const RUNTIME_KEY: &str = "sessionNeedsYou";
const MAX_TITLE_CHARS: usize = 200;
const MAX_DETAIL_CHARS: usize = 2_000;
const MAX_COMMAND_CHARS: usize = 2_000;
const MAX_ANSWER_CHARS: usize = 2_000;
const MAX_ANSWERS: usize = 6;
/// Open to-dos one session may hold; an agent that needs more is better off asking in one.
const MAX_OPEN: usize = 12;
/// Settled to-dos kept for `needs-you list --all`, newest last.
const SETTLED_KEEP: usize = 20;

/// One thing only the user can do.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct NeedsYouItem {
    /// Its question tag (`Q61`), from the session's own Q numbering.
    pub(crate) tag: String,
    /// What to do, as a short title.
    pub(crate) title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) detail: String,
    /// A command the user runs, which the row shows as a chip that copies it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) command: String,
    /// One-click answers in place of a single Done ("Works", "Something's off").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) answers: Vec<String>,
    pub(crate) asked_at: String,
    /// A turn end already alerted about it (`api::turn_ended`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) notified: bool,
}

/// A to-do that left the dock, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SettledItem {
    pub(crate) tag: String,
    pub(crate) title: String,
    /// `answered` (the user's Done, an answer or a Reply), `dismissed` (the user's Dismiss),
    /// `done` (the agent saw it happen) or `dropped` (the agent no longer needs it).
    pub(crate) outcome: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) answer: String,
    pub(crate) at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct NeedsYouRecord {
    /// The highest tag this session's to-dos ever took, so a tag is never given twice.
    pub(crate) last: u64,
    pub(crate) open: Vec<NeedsYouItem>,
    pub(crate) settled: Vec<SettledItem>,
}

impl NeedsYouRecord {
    pub(crate) fn of(session: &Value) -> Self {
        session
            .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
            .and_then(|record| serde_json::from_value(record.clone()).ok())
            .unwrap_or_default()
    }

    /// Takes the open to-do `tag` out, settled as `outcome`. `None` when none is open.
    pub(crate) fn settle(
        &mut self,
        tag: &str,
        outcome: &str,
        answer: &str,
        at: &str,
    ) -> Option<NeedsYouItem> {
        let index = self
            .open
            .iter()
            .position(|item| item.tag.eq_ignore_ascii_case(tag))?;
        let item = self.open.remove(index);
        self.settled.push(SettledItem {
            tag: item.tag.clone(),
            title: item.title.clone(),
            outcome: outcome.to_string(),
            answer: answer.chars().take(MAX_ANSWER_CHARS).collect(),
            at: at.to_string(),
        });
        let extra = self.settled.len().saturating_sub(SETTLED_KEEP);
        self.settled.drain(..extra);
        Some(item)
    }

    pub(crate) fn settled_of(&self, tag: &str) -> Option<&SettledItem> {
        self.settled
            .iter()
            .rev()
            .find(|item| item.tag.eq_ignore_ascii_case(tag))
    }
}

/// What `needs-you add` asks for.
pub(crate) struct NewItem {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) command: String,
    pub(crate) answers: Vec<String>,
}

impl NewItem {
    pub(crate) fn checked(self) -> Result<Self, DomainStateError> {
        let title = self.title.trim().to_string();
        if title.is_empty() {
            return Err(DomainStateError::bad_request(
                "Say what the user should do, as a short title.",
            ));
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "Keep the title under {MAX_TITLE_CHARS} characters and put the rest in --detail."
            )));
        }
        let answers: Vec<String> = self
            .answers
            .iter()
            .map(|answer| answer.trim().to_string())
            .filter(|answer| !answer.is_empty())
            .collect();
        if answers.len() > MAX_ANSWERS {
            return Err(DomainStateError::bad_request(format!(
                "Offer at most {MAX_ANSWERS} answers."
            )));
        }
        Ok(Self {
            title,
            detail: self.detail.trim().chars().take(MAX_DETAIL_CHARS).collect(),
            command: self
                .command
                .trim()
                .chars()
                .take(MAX_COMMAND_CHARS)
                .collect(),
            answers,
        })
    }
}

/// Adds `new` to `record` with the tag after `floor` (the session's highest Q tag anywhere else).
pub(crate) fn add(
    record: &mut NeedsYouRecord,
    new: NewItem,
    floor: u64,
    at: &str,
) -> Result<NeedsYouItem, DomainStateError> {
    if record.open.len() >= MAX_OPEN {
        return Err(DomainStateError::bad_request(format!(
            "This session already has {MAX_OPEN} open to-dos for the user. Clear the ones done with `ghostex needs-you done Q<n>` or `drop Q<n>` first."
        )));
    }
    let number = record.last.max(floor) + 1;
    record.last = number;
    let item = NeedsYouItem {
        tag: format!("Q{number}"),
        title: new.title,
        detail: new.detail,
        command: new.command,
        answers: new.answers,
        asked_at: at.to_string(),
        notified: false,
    };
    record.open.push(item.clone());
    Ok(item)
}

/// One writer at a time, so two `needs-you add` calls never take the same tag or lose a row.
pub(crate) fn write_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Saves `record` into the session's `runtimeSettings`. Returns whether a row changed.
pub(crate) fn write(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    record: &NeedsYouRecord,
) -> Result<bool, DomainStateError> {
    let value = serde_json::to_string(record).unwrap_or_else(|_| "{}".to_string());
    db.execute(
        &format!(
            "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.{RUNTIME_KEY}', json(?3)) WHERE projectId = ?1 AND sessionId = ?2"
        ),
        rusqlite::params![project_id, session_id, value],
    )
    .map(|changed| changed > 0)
    .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
}

/// How many to-dos wait on the user in this session (the pink dot counts them).
pub(crate) fn open_count(session: &Value) -> usize {
    session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/open"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// The open to-dos as a chat frame carries them (`needsYou`).
pub(crate) fn frame_value(record: &NeedsYouRecord) -> Value {
    json!({
        "items": record.open.iter().map(|item| json!({
            "tag": item.tag,
            "title": item.title,
            "detail": item.detail,
            "command": item.command,
            "answers": item.answers,
            "askedAt": item.asked_at,
        })).collect::<Vec<_>>(),
    })
}
