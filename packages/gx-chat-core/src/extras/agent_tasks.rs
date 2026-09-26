//! The agent's task plan, ported from
//! `packages/shared/session-chat-presentation/agent-tasks.ts`.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! apps/desktop/src/app/native_chat/agent_tasks.rs renders this projection; row order, the done
//! fold and the header line must not be recomputed in a renderer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::extras::activity::js_round;

/// `Number.MAX_SAFE_INTEGER`, the order an unnumbered task sorts under.
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Which of the three blocks a row sits in.
pub type TaskGroup = &'static str;

/// One row of the task panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskRow {
    pub id: String,
    pub group: String,
    pub subject: String,
    /// "waits for #3, #4" under a pending row, or "".
    pub blocked_label: String,
    /// Hover text: the blockers spelled out, else the subject.
    pub title: String,
}

/// The panel around those rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskPanel {
    /// "4 of 9 done", with the running task appended while collapsed.
    pub meta: String,
    /// Rows in running, waiting, done order, already folded.
    pub rows: Vec<AgentTaskRow>,
    /// 0 to 100, the header's progress bar.
    pub percent: i64,
    /// "3 more tasks" / "Show less tasks", or "" when nothing is folded.
    pub fold_label: String,
    /// Rows the fold is hiding right now.
    pub folded_count: usize,
}

/// `sessionChatAgentTaskPanel`: the whole panel, or `None` when the list is absent or empty.
pub fn agent_task_panel(
    tasks: Option<&Value>,
    collapsed: bool,
    show_completed: bool,
) -> Option<AgentTaskPanel> {
    let list: Vec<&Value> = tasks
        .and_then(|tasks| tasks.get("tasks"))
        .and_then(Value::as_array)
        .map(|list| list.iter().collect())
        .unwrap_or_default();
    if list.is_empty() {
        return None;
    }
    let running = sorted(&list, "in_progress");
    let waiting = sorted(&list, "pending");
    let done = sorted(&list, "completed");
    let done_count = done.len();
    let total = list.len();
    // Only OPEN tasks can still block: a finished blocker is no longer a wait. The lookup keeps
    // the list's own order, which is what the blocker label reads back.
    let subject_by_id: Vec<(String, String)> = list
        .iter()
        .filter(|task| task_group(task) != "completed")
        .map(|task| (task_id(task), task_subject(task)))
        .collect();
    // The latest completed task stays visible as the "just did" marker, like the CLI; the rest
    // fold behind the count until asked for.
    let latest_done = done.last().copied();
    let folded_done = done.len().saturating_sub(1);
    let visible_done: Vec<&Value> = if show_completed {
        done.clone()
    } else {
        latest_done.into_iter().collect()
    };
    let headline = running.first().or(waiting.first()).copied();
    let count_text = format!("{done_count} of {total} done");
    let headline_text = headline.map(|task| {
        if task.get("status").and_then(Value::as_str) == Some("in_progress") {
            task.get("activeForm")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| task_subject(task))
        } else {
            task_subject(task)
        }
    });
    let row = |task: &Value, group: &str| -> AgentTaskRow {
        let id = task_id(task);
        let blockers: Vec<String> = task
            .get("blockedBy")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .filter(|blocker| {
                        *blocker != id && subject_by_id.iter().any(|(known, _)| known == blocker)
                    })
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let blocked_title = if blockers.is_empty() {
            String::new()
        } else {
            format!(
                "Waits for {}",
                blockers
                    .iter()
                    .map(|blocker| {
                        let subject = subject_by_id
                            .iter()
                            .find(|(known, _)| known == blocker)
                            .map(|(_, subject)| subject.as_str())
                            .unwrap_or_default();
                        js_trim(&format!("#{blocker} {subject}")).to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let subject = task_subject(task);
        AgentTaskRow {
            id,
            group: group.to_string(),
            blocked_label: if group == "pending" && !blockers.is_empty() {
                format!("waits for #{}", blockers.join(", #"))
            } else {
                String::new()
            },
            title: if blocked_title.is_empty() {
                subject.clone()
            } else {
                blocked_title
            },
            subject,
        }
    };
    Some(AgentTaskPanel {
        // Only the collapsed header carries the running task: expanded, the rows below say it,
        // and saying it twice reads as a glitch.
        meta: match (&headline_text, collapsed) {
            (Some(text), true) if !text.is_empty() => format!("{count_text} \u{b7} {text}"),
            _ => count_text,
        },
        rows: running
            .iter()
            .map(|task| row(task, "in_progress"))
            .chain(waiting.iter().map(|task| row(task, "pending")))
            .chain(visible_done.iter().map(|task| row(task, "completed")))
            .collect(),
        percent: if total == 0 {
            0
        } else {
            js_round(done_count as f64 / total as f64 * 100.0) as i64
        },
        fold_label: if folded_done == 0 {
            String::new()
        } else if show_completed {
            "Show less tasks".to_string()
        } else {
            format!(
                "{folded_done} more task{}",
                if folded_done == 1 { "" } else { "s" }
            )
        },
        folded_count: folded_done,
    })
}

/// One group, in CLI order: the numeric id first, the raw id as the tie break.
fn sorted<'a>(list: &[&'a Value], group: TaskGroup) -> Vec<&'a Value> {
    let mut rows: Vec<&Value> = list
        .iter()
        .copied()
        .filter(|task| task_group(task) == group)
        .collect();
    // `Array.prototype.sort` is stable, and so is `sort_by`.
    rows.sort_by(|left, right| {
        task_order(left)
            .cmp(&task_order(right))
            .then_with(|| compare_strings(&task_id(left), &task_id(right)))
    });
    rows
}

/// `taskGroup`: anything the store does not name is pending.
fn task_group(task: &Value) -> TaskGroup {
    match task.get("status").and_then(Value::as_str) {
        Some("in_progress") => "in_progress",
        Some("completed") => "completed",
        _ => "pending",
    }
}

/// `taskOrder`: `Number.parseInt(id, 10)`, or `MAX_SAFE_INTEGER` when the id is not a number.
fn task_order(task: &Value) -> i64 {
    parse_int(&task_id(task)).unwrap_or(MAX_SAFE_INTEGER)
}

fn task_id(task: &Value) -> String {
    task.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn task_subject(task: &Value) -> String {
    task.get("subject")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `Number.parseInt(value, 10)`: the leading integer, ignoring whatever follows.
fn parse_int(value: &str) -> Option<i64> {
    let text = value.trim_start();
    let mut characters = text.chars().peekable();
    let mut digits = String::new();
    if matches!(characters.peek(), Some('+') | Some('-')) {
        digits.push(characters.next().expect("peeked"));
    }
    for character in characters {
        if character.is_ascii_digit() {
            digits.push(character);
        } else {
            break;
        }
    }
    if digits.is_empty() || digits == "+" || digits == "-" {
        return None;
    }
    digits.parse().ok()
}

/// `String.prototype.localeCompare` for the ASCII task ids the CLI writes, which is a code-unit
/// comparison for everything this list carries.
fn compare_strings(left: &str, right: &str) -> std::cmp::Ordering {
    left.cmp(right)
}

/// `String.prototype.trim`: Unicode `White_Space` plus the byte order mark, which JavaScript
/// counts as whitespace and Rust's `char::is_whitespace` does not.
pub fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}
