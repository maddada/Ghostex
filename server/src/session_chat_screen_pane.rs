/*
CDXC:SessionChatTerminalActivity 2026-09-04 WHY:
Claude Code's `/diff` pane paints a second column on the SAME screen rows as
the conversation: the file list, its dividers and the selected file's diff
start at one fixed column (112 of 200 on the screen this was found on), and
the conversation wraps to the left of it. Read row by row, every status looked
like "…rows plus server/src/session_chat_notice.rs +13 the pen-icon…", and
pane-only rows (a divider, "↓ 5 more below") started far enough right to pass
for wrapped continuation rows. Every screen detector reads the same rows, so
the pane is cut once here, before any of them look.

The pane is a vertical rectangle, so its rows all start text at the same column
after a gap of three or more spaces. When enough rows agree on a column, every
row is cut there; a screen without a pane has no such agreement and is left
untouched. No wording of the pane is matched.
*/

use std::collections::HashMap;

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// A pane starts after at least this many spaces of gap.
const PANE_GAP_MIN_SPACES: usize = 3;
/// A pane never starts this close to the left edge; a wrapped list item does.
const PANE_MIN_COLUMN: usize = 24;
/// Rows that must agree on the column before it counts as a pane.
const PANE_MIN_ROWS: usize = 4;
/// Wide glyphs left of the pane shift its char index a little on that row.
const PANE_COLUMN_TOLERANCE: usize = 4;

/// The screen with a side pane's column removed from every row, or the screen
/// as captured when no pane is painted.
pub(crate) fn strip_side_pane(screen_text: &str) -> String {
    let lines: Vec<String> = screen_text
        .lines()
        .map(|raw| normalize_spaces(&strip_ansi_sgr(raw)))
        .collect();
    let Some(column) = side_pane_column(&lines) else {
        return screen_text.to_string();
    };
    lines
        .iter()
        .map(|line| cut_at_pane(line, column))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Char indices where text begins after a run of spaces long enough to be a
/// column gap, ignoring the left part of the screen where prose wraps.
fn text_starts_after_gap(line: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut spaces = 0;
    for (index, ch) in line.chars().enumerate() {
        if ch == ' ' {
            spaces += 1;
            continue;
        }
        if spaces >= PANE_GAP_MIN_SPACES && index >= PANE_MIN_COLUMN {
            starts.push(index);
        }
        spaces = 0;
    }
    starts
}

fn side_pane_column(lines: &[String]) -> Option<usize> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for line in lines {
        for start in text_starts_after_gap(line) {
            *counts.entry(start).or_default() += 1;
        }
    }
    let (&column, &count) = counts
        .iter()
        .max_by_key(|(column, count)| (**count, std::cmp::Reverse(**column)))?;
    if count < PANE_MIN_ROWS {
        return None;
    }
    // The pane's own left edge may hold fewer rows than an inner column of
    // it (diff line numbers, indented list rows); take the leftmost column
    // near the winner that enough rows share.
    let mut leftmost = column;
    for candidate in (column.saturating_sub(PANE_COLUMN_TOLERANCE)..column).rev() {
        if counts.get(&candidate).copied().unwrap_or(0) >= PANE_MIN_ROWS {
            leftmost = candidate;
        }
    }
    Some(leftmost)
}

fn cut_at_pane(line: &str, column: usize) -> String {
    let low = column.saturating_sub(PANE_COLUMN_TOLERANCE);
    let high = column + PANE_COLUMN_TOLERANCE;
    let cut = text_starts_after_gap(line)
        .into_iter()
        .find(|start| (low..=high).contains(start))
        .or_else(|| {
            // A pane row whose text starts further right than the tolerance
            // (an indented diff line) still has nothing but gap at the edge.
            let chars: Vec<char> = line.chars().collect();
            (chars.len() > column && chars[low..=column].iter().all(|ch| *ch == ' '))
                .then_some(column)
        });
    match cut {
        Some(cut) => line
            .chars()
            .take(cut)
            .collect::<String>()
            .trim_end()
            .to_string(),
        None => line.trim_end().to_string(),
    }
}
