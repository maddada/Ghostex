//! What of `MEMORY.md` loads into a bot: 200 lines and 24 KB. When a write would pass that, the
//! oldest dated notes move to `memory/archive.md` (notes struck through first, then notes past their
//! `until` date, then the oldest), so the newest facts keep loading. Lines without a date (written
//! by hand, headings) never move.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! The budget is enforced when gxserver writes, not when it reads: a bot that edits MEMORY.md with its own file tools can pass it, and then the brief loads only the first 200 lines / 24 KB and the Memory tab's gauge shows red until the next write through Ghostex makes room. Moving notes silently on a read would change the user's file without a journal entry to undo.

use chrono::NaiveDate;

use super::entries::{expired, parse, struck_through};

pub(crate) const MEMORY_MAX_LINES: usize = 200;
pub(crate) const MEMORY_MAX_BYTES: usize = 24 * 1024;
/// The gauge turns amber from this share of the budget.
const AMBER_FROM: f64 = 0.8;

/// How full `MEMORY.md` is against its budget.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Gauge {
    pub lines: usize,
    pub bytes: usize,
}

impl Gauge {
    pub fn of(text: &str) -> Self {
        Self {
            lines: text.lines().count(),
            bytes: text.len(),
        }
    }

    pub fn fits(self) -> bool {
        self.lines <= MEMORY_MAX_LINES && self.bytes <= MEMORY_MAX_BYTES
    }

    /// The fuller of the two measures, as a share of its limit.
    pub fn share(self) -> f64 {
        (self.lines as f64 / MEMORY_MAX_LINES as f64)
            .max(self.bytes as f64 / MEMORY_MAX_BYTES as f64)
    }

    /// `ok`, `amber` (80% or more) or `red` (over the budget: the rest does not load).
    pub fn level(self) -> &'static str {
        if !self.fits() {
            "red"
        } else if self.share() >= AMBER_FROM {
            "amber"
        } else {
            "ok"
        }
    }
}

/// The first `max_lines` lines and `max_bytes` bytes of `text`, and whether it was cut.
pub(crate) fn within_budget(text: &str, max_lines: usize, max_bytes: usize) -> (String, bool) {
    let mut kept = String::new();
    let mut cut = false;
    for (index, line) in text.lines().enumerate() {
        if index >= max_lines || kept.len() + line.len() + 1 > max_bytes {
            cut = true;
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    (kept.trim_end().to_string(), cut)
}

/// `MEMORY.md` made to fit: the text that stays and the notes moved out (struck through, oldest
/// first), or `None` when it already fits. The text that stays may still not fit when the lines
/// without a date alone pass the budget.
pub(crate) fn make_room(text: &str, today: NaiveDate) -> Option<(String, Vec<String>)> {
    if Gauge::of(text).fits() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut order = parse(text);
    // Struck through first, then past their date, then the oldest; the file's order breaks ties.
    order.sort_by_key(|entry| {
        let rank = if entry.struck {
            0
        } else if expired(entry, today) {
            1
        } else {
            2
        };
        (rank, entry.date, entry.start)
    });
    let mut moved_ranges = Vec::new();
    let mut gauge = Gauge::of(text);
    for entry in order {
        if gauge.fits() {
            break;
        }
        let removed_lines = entry.end - entry.start;
        let removed_bytes: usize = lines[entry.start..entry.end]
            .iter()
            .map(|line| line.len() + 1)
            .sum();
        gauge.lines -= removed_lines;
        gauge.bytes = gauge.bytes.saturating_sub(removed_bytes);
        moved_ranges.push((entry.start, entry.end));
    }
    if moved_ranges.is_empty() {
        return Some((text.to_string(), Vec::new()));
    }
    moved_ranges.sort();
    let moved = moved_ranges
        .iter()
        .map(|(start, end)| struck_through(&lines[*start..*end]))
        .collect();
    let mut kept = Vec::with_capacity(lines.len());
    let mut next = moved_ranges.iter().peekable();
    let mut index = 0;
    while index < lines.len() {
        if let Some((start, end)) = next.peek() {
            if index == *start {
                index = *end;
                next.next();
                continue;
            }
        }
        kept.push(lines[index]);
        index += 1;
    }
    let mut kept = kept.join("\n");
    kept.push('\n');
    Some((kept, moved))
}
