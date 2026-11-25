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
    /// RFC3339 timestamp until which this to-do is snoozed (Q4686 A). Additive: older clients ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) snoozed_until: Option<String>,
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
    /// Preserved from the open item so Mark all read Undo can restore a dismissed to-do intact.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) detail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) answers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) asked_at: String,
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
            detail: item.detail.clone(),
            command: item.command.clone(),
            answers: item.answers.clone(),
            asked_at: item.asked_at.clone(),
        });
        let extra = self.settled.len().saturating_sub(SETTLED_KEEP);
        self.settled.drain(..extra);
        Some(item)
    }

    /// Snoozes an open to-do until `until` (RFC3339), or clears its snooze when `None`.
    pub(crate) fn set_snoozed(&mut self, tag: &str, until: Option<String>) -> bool {
        let Some(item) = self
            .open
            .iter_mut()
            .find(|item| item.tag.eq_ignore_ascii_case(tag))
        else {
            return false;
        };
        item.snoozed_until = until;
        true
    }

    /// Restores a recently dismissed to-do back into `open` (Mark all read Undo, Q4685 A).
    pub(crate) fn restore(&mut self, tag: &str) -> bool {
        if self
            .open
            .iter()
            .any(|item| item.tag.eq_ignore_ascii_case(tag))
        {
            return false;
        }
        let Some(index) = self
            .settled
            .iter()
            .rposition(|item| item.tag.eq_ignore_ascii_case(tag) && item.outcome == "dismissed")
        else {
            return false;
        };
        let settled = self.settled.remove(index);
        self.open.push(NeedsYouItem {
            tag: settled.tag,
            title: settled.title,
            detail: settled.detail,
            command: settled.command,
            answers: settled.answers,
            asked_at: if settled.asked_at.is_empty() {
                settled.at
            } else {
                settled.asked_at
            },
            notified: true,
            snoozed_until: None,
        });
        true
    }

    pub(crate) fn settled_of(&self, tag: &str) -> Option<&SettledItem> {
        self.settled
            .iter()
            .rev()
            .find(|item| item.tag.eq_ignore_ascii_case(tag))
    }
}

/// First words of a to-do title that ask the user to paste, post, send or run something.
const ACT_VERBS: &[&str] = &[
    "paste", "post", "copy", "send", "reply", "run", "type", "enter", "sign in", "open",
];

/// Why a to-do (`needs-you add`, `item ask`) that asks the user to paste, post or run something
/// is refused, or `None` when it may stand: it holds nothing to act on.
///
/// CDXC:Coordinators 2026-11-25 DECISION:
/// A coordinator added "Paste the approved reply to Ajax in #general" with a one-line detail and
/// put the reply and its link only in its chat answer; the user reads Needs you on its own, often
/// on the phone, so the to-do was useless there. User: "there's a description field for these
/// needs you/pings WHY NOT USE IT". On the needs-you-detail page (Q4860 "Refuse") the user chose a
/// refusal over a warning, since the playbook rule alone had not held. So a to-do (no answers, or
/// only Done) whose title starts with paste, post, copy, send, reply, run, type, enter, sign in or
/// open is refused unless its detail holds a ``` block, a link or inline `code`, or it has a
/// command. A decision question with real answers ("Send the release?" Send / Hold) passes.
/// SEE-ALSO: server/src/coordinators/work.rs (`handle_work_request` "ask" arm),
/// server/src/coordinators/user_handoff.rs (the guard this one is modelled on),
/// server/src/coordinators/playbook.md ("Asking the user", Description bullet).
pub(crate) fn paste_refusal(
    title: &str,
    detail: &str,
    command: &str,
    answers: &[String],
) -> Option<String> {
    let is_todo = answers
        .iter()
        .map(|answer| answer.trim().trim_start_matches('*').trim())
        .filter(|answer| !answer.is_empty())
        .all(|answer| answer.eq_ignore_ascii_case("done"));
    if !is_todo || !command.trim().is_empty() {
        return None;
    }
    let words: Vec<String> = title
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(2)
        .map(str::to_lowercase)
        .collect();
    let first = words.first()?;
    let two = words.join(" ");
    ACT_VERBS
        .iter()
        .find(|verb| **verb == first.as_str() || **verb == two.as_str())?;
    let has_link = detail.contains("http://") || detail.contains("https://");
    let has_code = detail.contains("```") || detail.matches('`').count() >= 2;
    if has_link || has_code {
        return None;
    }
    let lead = title.split_whitespace().next().unwrap_or(title);
    Some(format!(
        "\"{lead} …\" asks the user to act, but --detail holds no text, link or command for it. Put the text to paste in a ``` block in --detail (and the link where it goes), or the link or `command` itself, or pass --command, so the user can do it from Details."
    ))
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
        if let Some(refusal) = paste_refusal(&title, &self.detail, &self.command, &answers) {
            return Err(DomainStateError::bad_request(refusal));
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
        snoozed_until: None,
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

/// Whether `until` is a future RFC3339 timestamp right now.
pub(crate) fn is_snooze_active(until: Option<&str>) -> bool {
    let Some(until) = until.filter(|s| !s.is_empty()) else {
        return false;
    };
    chrono::DateTime::parse_from_rfc3339(until)
        .map(|dt| dt.with_timezone(&chrono::Utc) > chrono::Utc::now())
        .unwrap_or(false)
}

/// How many to-dos wait on the user in this session (the pink dot counts them; snoozed ones stay out).
pub(crate) fn open_count(session: &Value) -> usize {
    session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/open"))
        .and_then(Value::as_array)
        .map_or(0, |items| {
            items
                .iter()
                .filter(|item| !is_snooze_active(item.get("snoozedUntil").and_then(Value::as_str)))
                .count()
        })
}

/// The open to-dos (and recently settled ones for the Answered drawer) as a chat frame carries them (`needsYou`).
pub(crate) fn frame_value(record: &NeedsYouRecord) -> Value {
    json!({
        "items": record.open.iter().map(|item| {
            let mut value = json!({
                "tag": item.tag,
                "title": item.title,
                "detail": item.detail,
                "command": item.command,
                "answers": item.answers,
                "askedAt": item.asked_at,
            });
            if is_snooze_active(item.snoozed_until.as_deref()) {
                if let Some(until) = &item.snoozed_until {
                    value["snoozedUntil"] = json!(until);
                }
            }
            value
        }).collect::<Vec<_>>(),
        "settled": record.settled.iter().map(|item| json!({
            "tag": item.tag,
            "title": item.title,
            "outcome": item.outcome,
            "answer": item.answer,
            "at": item.at,
        })).collect::<Vec<_>>(),
    })
}
