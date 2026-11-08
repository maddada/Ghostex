//! Dated notes in a bot's memory files: `- 2026-11-08 · Prefers aisle seats · until 2026-12-01`.
//! A note may run over several lines (the later ones indented two spaces) and ends with an
//! optional `· until <date>`; a note struck through (`- ~~2026-11-08 · …~~`) is one the user marked
//! as no longer true. Every other line (headings, lines written by hand) is plain text Ghostex
//! never moves.

use chrono::NaiveDate;

/// A note's text: at most this many characters and lines.
pub(crate) const ENTRY_MAX_CHARS: usize = 1_000;
pub(crate) const ENTRY_MAX_LINES: usize = 20;

const SEPARATOR: &str = " · ";
const UNTIL: &str = " · until ";

/// One dated note: its lines `start..end` in the file.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub start: usize,
    pub end: usize,
    pub date: NaiveDate,
    pub until: Option<NaiveDate>,
    pub struck: bool,
    /// The note's words without the date, the `until` and the strike marks, on one line.
    pub text: String,
}

fn date_at(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

/// The dated note starting at `line`, if it starts one: `(date, struck, rest after the date)`.
fn entry_head(line: &str) -> Option<(NaiveDate, bool, &str)> {
    let rest = line.strip_prefix("- ")?;
    let (struck, rest) = match rest.strip_prefix("~~") {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let date = date_at(rest)?;
    let after = rest.get(10..)?;
    let words = after
        .strip_prefix(SEPARATOR)
        .or_else(|| after.strip_prefix(" "))?;
    Some((date, struck, words))
}

fn is_continuation(line: &str) -> bool {
    (line.starts_with("  ") || line.starts_with('\t')) && !line.trim().is_empty()
}

/// The dated notes in `text`, in file order.
pub(crate) fn parse(text: &str) -> Vec<Entry> {
    let lines: Vec<&str> = text.lines().collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((date, struck, first)) = entry_head(lines[index]) else {
            index += 1;
            continue;
        };
        let start = index;
        index += 1;
        while index < lines.len() && is_continuation(lines[index]) {
            index += 1;
        }
        let mut words = std::iter::once(first)
            .chain(lines[start + 1..index].iter().map(|line| line.trim()))
            .collect::<Vec<_>>()
            .join(" ");
        if struck {
            words = words.trim_end().trim_end_matches("~~").to_string();
        }
        let mut until = None;
        if let Some(at) = words.rfind(UNTIL) {
            if let Some(date) = date_at(&words[at + UNTIL.len()..]) {
                if words[at + UNTIL.len() + 10..].trim().is_empty() {
                    until = Some(date);
                    words.truncate(at);
                }
            }
        }
        entries.push(Entry {
            start,
            end: index,
            date,
            until,
            struck,
            text: words.trim().to_string(),
        });
    }
    entries
}

/// The note's text as the user gave it, or the sentence saying why it can't be kept.
pub(crate) fn check_text(text: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .skip_while(|line| line.trim().is_empty())
        .collect();
    let text = lines.join("\n").trim().to_string();
    if text.is_empty() {
        return Err("The note is empty.".to_string());
    }
    if text.chars().count() > ENTRY_MAX_CHARS {
        return Err(format!(
            "Keep a note under {ENTRY_MAX_CHARS} characters; put longer detail in a topic (--topic <name>)."
        ));
    }
    if text.lines().count() > ENTRY_MAX_LINES {
        return Err(format!(
            "Keep a note to {ENTRY_MAX_LINES} lines; put longer detail in a topic (--topic <name>)."
        ));
    }
    Ok(text)
}

/// A note as it is written into a file: dated `date`, kept until `until` when set.
pub(crate) fn format_entry(date: NaiveDate, text: &str, until: Option<NaiveDate>) -> String {
    let mut lines = text.lines();
    let mut out = format!(
        "- {}{SEPARATOR}{}",
        date.format("%Y-%m-%d"),
        lines.next().unwrap_or_default().trim()
    );
    for line in lines {
        out.push_str("\n  ");
        out.push_str(line.trim());
    }
    if let Some(until) = until {
        out.push_str(&format!("{UNTIL}{}", until.format("%Y-%m-%d")));
    }
    out
}

/// The same note struck through, for the archive.
pub(crate) fn struck_through(lines: &[&str]) -> String {
    let mut out = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let mut line = if index == 0 {
            match line.strip_prefix("- ") {
                Some(rest) if !rest.starts_with("~~") => format!("- ~~{rest}"),
                _ => line.to_string(),
            }
        } else {
            line.to_string()
        };
        if index + 1 == lines.len() && !line.trim_end().ends_with("~~") {
            line = format!("{}~~", line.trim_end());
        }
        out.push(line);
    }
    out.join("\n")
}

/// Whether `entry`'s note is past its `until` date.
pub(crate) fn expired(entry: &Entry, today: NaiveDate) -> bool {
    entry.until.is_some_and(|until| until < today)
}
