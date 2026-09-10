//! CDXC:SessionStatus 2026-09-10 WHY:
//! Claude keeps an idle child in its footer between monitor events. Hiding it at every end_turn made maestro-swift-setup's watchdog disappear from chat while it was still listed in the terminal.
//! Match the footer's lifetime counter against the original launch and the latest idle boundary, not the current time or resumed turn start. A frozen counter must still identify the same child minutes later.

use crate::session_chat_agent_fleet::{
    agent_fleet_block_start, normalized_screen_lines, SessionChatSubAgent,
};
use crate::session_chat_terminal_activity::parse_elapsed_seconds;

pub(crate) struct ClaudeChild {
    pub agent: SessionChatSubAgent,
    pub launched_at: Option<i64>,
    pub sampled_at: i64,
}

struct Progress<'a> {
    name: &'a str,
    task: &'a str,
    elapsed_seconds: u64,
    tokens: Option<&'a str>,
}

fn progress(line: &str) -> Option<Progress<'_>> {
    let line = line.trim_start_matches('❯').trim_start();
    let mut chars = line.chars();
    let marker = chars.next()?;
    if marker.is_alphanumeric() || marker.is_whitespace() {
        return None;
    }
    let content = chars.as_str().strip_prefix(' ')?.trim_start();
    let (columns, tokens) = content
        .rsplit_once(" · ")
        .map_or((content, None), |(left, right)| (left, Some(right.trim())));
    let (name, task) = columns.split_once("  ")?;
    let name = name.split_once(" (+").map_or(name, |(name, _)| name).trim();
    let task = task.trim();
    for (start, _) in task.char_indices() {
        if start > 0 && !task[..start].ends_with(' ') {
            continue;
        }
        if let Some(elapsed_seconds) = parse_elapsed_seconds(&task[start..]) {
            return Some(Progress {
                name,
                task: task[..start].trim_end(),
                elapsed_seconds,
                tokens,
            });
        }
    }
    None
}

/// CDXC:SessionStatus 2026-09-10 DECISION:
/// User: Claude subagents listed at the bottom of the terminal should also appear in the chat Subagents card.
pub(crate) fn reconcile(
    mut children: Vec<ClaudeChild>,
    screen: Option<&str>,
) -> Vec<SessionChatSubAgent> {
    let lines = normalized_screen_lines(screen.unwrap_or_default());
    let rows: Vec<_> = agent_fleet_block_start(&lines)
        .into_iter()
        .flat_map(|start| lines[start + 1..].iter().filter_map(|line| progress(line)))
        .collect();
    let matches = |row: &Progress<'_>, child: &ClaudeChild| {
        child.agent.name.eq_ignore_ascii_case(row.name)
            && child.launched_at.is_some_and(|launched_at| {
                child
                    .sampled_at
                    .saturating_sub((row.elapsed_seconds as i64).saturating_mul(1000))
                    .abs_diff(launched_at)
                    <= 2_000
            })
    };
    let mut visible = vec![false; children.len()];
    for row in &rows {
        let candidates: Vec<_> = children
            .iter()
            .enumerate()
            .filter(|(_, child)| matches(row, child))
            .map(|(index, _)| index)
            .collect();
        let [index] = candidates.as_slice() else {
            continue;
        };
        if rows
            .iter()
            .filter(|other| matches(other, &children[*index]))
            .count()
            != 1
        {
            continue;
        }
        visible[*index] = true;
        let child = &mut children[*index].agent;
        if !row.task.is_empty() {
            child.task = Some(row.task.to_string());
        }
        child.elapsed_seconds = Some(row.elapsed_seconds);
        child.tokens = row
            .tokens
            .filter(|tokens| !tokens.is_empty())
            .map(str::to_string);
    }
    children
        .into_iter()
        .zip(visible)
        .filter_map(|(child, visible)| (child.agent.working || visible).then_some(child.agent))
        .collect()
}
