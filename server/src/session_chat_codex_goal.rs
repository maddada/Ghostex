//! The goal cell Codex prints for `/goal`, lifted out of its local command output.

use serde_json::{json, Map, Value};

/// CDXC:SessionChat 2026-09-06 WHY:
/// Codex prints a `/goal` result as one info cell, `• Goal <status> Objective: <text>`, and then keeps working, so the screen diff behind the local command output grew with the whole agent turn.
/// Only the goal cell belongs to the command; the diff is cut at the next cell and the row stops refreshing once that next cell exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatCodexGoal {
    /// Codex's own label: `active`, `paused`, `stalled`, `usage limited`, `limited by budget`, `complete`, `cleared`.
    pub status: String,
    /// The objective text exactly as Codex echoed it, without the trailing usage summary.
    pub objective: String,
    /// `Time: 2m · Tokens: 63.9K/50K`, when Codex appended usage.
    pub usage: Option<String>,
}

impl SessionChatCodexGoal {
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("status".to_string(), json!(self.status));
        map.insert("objective".to_string(), json!(self.objective));
        if let Some(usage) = self.usage.as_deref() {
            map.insert("usage".to_string(), json!(usage));
        }
        Value::Object(map)
    }

    pub fn identity(&self) -> String {
        format!(
            "{}\u{1d}{}\u{1d}{}",
            self.status,
            self.objective,
            self.usage.as_deref().unwrap_or_default()
        )
    }
}

pub(crate) struct CodexGoalCell {
    pub goal: SessionChatCodexGoal,
    /// The cell's lines, for the plain output record.
    pub text: String,
    /// A later cell has started, so the goal cell can no longer change.
    pub settled: bool,
}

pub(crate) fn command_is_codex_goal(command: &str) -> bool {
    command.split_whitespace().next() == Some("/goal")
}

fn starts_new_cell(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with('•')
        || line.starts_with('■')
        || line.starts_with('›')
        || line.starts_with('─')
        || line.starts_with("Usage: /goal")
}

/// The first goal cell in a `/goal` command's screen output.
pub(crate) fn parse_codex_goal_cell(output: &str) -> Option<CodexGoalCell> {
    let lines: Vec<&str> = output.lines().collect();
    let start = lines.iter().position(|line| {
        line.trim_start()
            .strip_prefix('•')
            .is_some_and(|rest| rest.trim_start().starts_with("Goal "))
    })?;
    let mut end = lines.len();
    let mut settled = false;
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        if starts_new_cell(line) {
            end = index;
            settled = true;
            break;
        }
    }
    let cell: Vec<&str> = lines[start..end]
        .iter()
        .map(|line| line.trim_end())
        .collect();
    let text = cell.join("\n").trim().to_string();
    let head = cell[0]
        .trim_start()
        .strip_prefix('•')
        .unwrap_or(cell[0])
        .trim_start()
        .strip_prefix("Goal ")
        .unwrap_or_default();
    let (status, first_objective_line) = match head.split_once(" Objective: ") {
        Some((status, objective)) => (status.trim(), Some(objective)),
        None => (head.trim(), None),
    };
    if status.is_empty() {
        return None;
    }
    let mut objective = String::new();
    if let Some(first) = first_objective_line {
        objective.push_str(first);
        for line in &cell[1..] {
            objective.push('\n');
            objective.push_str(line);
        }
    }
    let (objective, usage) = split_trailing_usage(objective.trim());
    Some(CodexGoalCell {
        goal: SessionChatCodexGoal {
            status: status.to_string(),
            objective,
            usage,
        },
        text,
        settled,
    })
}

/// `… Time: 2m. Tokens: 63.9K/50K.` off the end of the objective, usage last.
fn split_trailing_usage(objective: &str) -> (String, Option<String>) {
    let mut text = objective.trim_end();
    let mut parts: Vec<String> = Vec::new();
    for key in ["Tokens: ", "Time: "] {
        let Some(index) = text.rfind(key) else {
            continue;
        };
        let tail = &text[index + key.len()..];
        let boundary_ok = index == 0 || text[..index].ends_with(char::is_whitespace);
        let tail_ok = tail.ends_with('.')
            && tail.starts_with(|ch: char| ch.is_ascii_digit())
            && tail.len() <= 40
            && tail
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | ',' | '/'));
        if !boundary_ok || !tail_ok {
            continue;
        }
        parts.push(format!("{key}{}", tail.trim_end_matches('.')));
        text = text[..index].trim_end();
    }
    parts.reverse();
    let usage = (!parts.is_empty()).then(|| parts.join(" · "));
    (text.to_string(), usage)
}
