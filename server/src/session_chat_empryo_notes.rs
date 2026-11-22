//! The notice rows of an Empryo chat: model and effort changes, Empryo's own `system` rows and
//! the events read off its screen (session_chat_empryo_events.rs), placed inside the turn they
//! happened in (session_chat_empryo_mirror.rs).
//!
//! CDXC:SessionChat 2026-10-07 DECISION:
//! Sven wants Empryo's events in the chat as quiet notice rows inside the turn, in the order Empryo shows them, and a model or effort change (from the chat pill, `/models`, `/effort` or Empryo's own keys) as a status row like Claude's model and effort rows.
//!
//! CDXC:SessionChat 2026-10-07 WHY:
//! Empryo 3.9.1-beta logs no line for a model or effort change; the tab patches it writes carry the tab's whole `activeModel` and `effortByFamily` (`null` while the tab keeps the configured default) after every turn and title change, so a change is a value that differs from the tab's previous one. Its events ("Waiting for Genome…", the safety-buffering notice) live only in the TUI's memory, which is why they are read off the screen.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/noise.rs (`is_setting_status`)

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::session_chat::extract_string;

/// How far apart Ghostex's model picker logs the model and then the effort it sets with it.
const MODEL_EFFORT_FOLD_MS: i64 = 5_000;

pub(crate) struct EmpryoNote {
    pub(crate) id: String,
    pub(crate) at: i64,
    pub(crate) text: String,
    /// The model a change set, which an effort change logged right after it folds into.
    model: Option<String>,
}

#[derive(Default)]
pub(crate) struct EmpryoTabNotes {
    model: Option<String>,
    effort: Option<BTreeMap<String, String>>,
    pub(crate) notes: Vec<EmpryoNote>,
}

/// `subscriptions/gpt-6.1-sol` reads as the chat's model pill does: `gpt-6.1-sol`.
fn model_name(model: &str) -> &str {
    model.rsplit('/').next().unwrap_or(model)
}

/// The effort level stays raw; the chat core words it as every effort in chat is worded.
fn model_with_effort(model: &str, level: &str) -> String {
    format!("Set model to {} with {level} effort", model_name(model))
}

impl EmpryoTabNotes {
    fn push(&mut self, tab: &str, at: i64, text: String, model: Option<String>) {
        let id = format!("empryo:note:{tab}:{}", self.notes.len());
        self.notes.push(EmpryoNote {
            id,
            at,
            text,
            model,
        });
    }

    /// One `tab` patch: a model or effort that differs from the tab's previous one is a change.
    pub(crate) fn observe_patch(&mut self, tab: &str, patch: &Value, at: i64) {
        let mut model_change = None;
        if let Some(model) = patch
            .get("activeModel")
            .and_then(Value::as_str)
            .filter(|model| !model.is_empty() && *model != "none")
        {
            if self
                .model
                .as_deref()
                .is_some_and(|previous| previous != model)
            {
                model_change = Some(model.to_string());
            }
            self.model = Some(model.to_string());
        }
        let mut effort_change = None;
        if let Some(value) = patch.get("effortByFamily") {
            let effort: BTreeMap<String, String> = value
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(family, level)| Some((family.clone(), level.as_str()?.to_string())))
                .collect();
            if let Some(previous) = self.effort.as_ref().filter(|previous| **previous != effort) {
                // The family whose level moved; an override that went away is the default again.
                effort_change = Some(
                    effort
                        .iter()
                        .find(|(family, level)| previous.get(*family) != Some(level))
                        .map_or_else(|| "default".to_string(), |(_, level)| level.clone()),
                );
            }
            self.effort = Some(effort);
        }
        match (model_change, effort_change) {
            (Some(model), Some(level)) => {
                self.push(tab, at, model_with_effort(&model, &level), None)
            }
            (Some(model), None) => self.push(
                tab,
                at,
                format!("Set model to {}", model_name(&model)),
                Some(model),
            ),
            (None, Some(level)) => {
                // Ghostex's model picker logs the model and its effort as two patches a moment
                // apart; they read as one change, as Claude's pill does.
                if let Some(note) = self.notes.last_mut().filter(|note| {
                    note.model.is_some() && at.saturating_sub(note.at) <= MODEL_EFFORT_FOLD_MS
                }) {
                    let model = note.model.take().unwrap_or_default();
                    note.text = model_with_effort(&model, &level);
                } else {
                    self.push(tab, at, format!("Set effort level to {level}"), None);
                }
            }
            (None, None) => {}
        }
    }

    /// A `system` record, which Empryo wrote for its chat rows before 3.9.
    pub(crate) fn observe_system(&mut self, tab: &str, ui: &Map<String, Value>, at: i64) {
        let Some(text) = extract_string(ui.get("content"))
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
        else {
            return;
        };
        self.push(tab, at, text, None);
    }

    /// A rewind dropped the turns from `from` on, and the notes made during them with them.
    pub(crate) fn cut(&mut self, from: i64) {
        self.notes.retain(|note| note.at < from);
    }
}

pub(crate) fn notice_row(id: &str, turn: Option<&str>, at: i64, text: &str) -> Value {
    json!({ "row": "notice", "id": id, "turn": turn, "ts": at, "text": text })
}

/// The newest of a turn's calls (id, start) that started by `at`, which a note made then follows.
pub(crate) fn anchor_at(calls: &[(String, i64)], at: i64) -> Option<String> {
    calls
        .iter()
        .rfind(|(_, started)| *started <= at)
        .map(|(id, _)| id.clone())
}

fn row_calls(row: &Value) -> Vec<&str> {
    row.get("toolCalls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|call| call.get("callId")?.as_str())
        .collect()
}

/// Where a note anchored to `call` goes: after the tool row holding the call and the result rows
/// right after it, so it never splits a call from its result. A call the rows do not hold puts it
/// after the turn's last tool row; no call puts it at `body_start`, the top of the turn.
fn notice_position(rows: &[Value], body_start: usize, call: Option<&str>) -> usize {
    let is_tool_row = |row: &Value| {
        row.get("row").and_then(Value::as_str) == Some("tool") || !row_calls(row).is_empty()
    };
    let Some(call) = call else {
        return body_start;
    };
    let holder = rows[body_start..]
        .iter()
        .position(|row| row_calls(row).contains(&call))
        .map(|at| body_start + at);
    let Some(holder) = holder else {
        return rows[body_start..]
            .iter()
            .rposition(is_tool_row)
            .map_or(body_start, |at| body_start + at + 1);
    };
    let group = row_calls(&rows[holder]);
    let mut position = holder + 1;
    while rows.get(position).is_some_and(|row| {
        row.get("row").and_then(Value::as_str) == Some("tool")
            && row
                .get("callId")
                .and_then(Value::as_str)
                .is_some_and(|id| group.contains(&id))
    }) {
        position += 1;
    }
    position
}

/// Inserts a turn's notes (anchor call, row), in their order, each after its anchor's tool group.
///
/// CDXC:SessionChat 2026-10-07 WHY:
/// The chat orders rows by timestamp before byte offset, and Empryo logs a prompt only after its Genome wait and model brief, so a note stamped with its own time (the effort change Sven made during the wait) sorted above the prompt it belongs under. A note takes the time of the row it follows, which keeps it where it was placed.
pub(crate) fn insert_notices(
    rows: &mut Vec<Value>,
    body_start: usize,
    notices: Vec<(Option<String>, Value)>,
) {
    let mut placed: Vec<(usize, usize, Value)> = notices
        .into_iter()
        .enumerate()
        .map(|(order, (call, row))| {
            (
                notice_position(rows, body_start, call.as_deref()),
                order,
                row,
            )
        })
        .collect();
    // From the bottom up, so each insert leaves the positions above it valid; notes sharing a
    // position keep their order.
    placed.sort_by(|left, right| right.0.cmp(&left.0).then(right.1.cmp(&left.1)));
    for (position, _, mut row) in placed {
        if let Some(at) = position
            .checked_sub(1)
            .and_then(|before| rows[before].get("ts"))
            .filter(|at| !at.is_null())
        {
            row["ts"] = at.clone();
        }
        rows.insert(position, row);
    }
}
