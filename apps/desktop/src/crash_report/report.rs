//! The issue a crash becomes: its title, and the body the crash window shows under "What's sent"
//! exactly as the feedback service posts it (the service adds only its one footer line).

use super::{FeedbackScrubContext, scrub_personal_data};

use super::record::{CrashKind, CrashRecord};
use super::system::{SystemFacts, build_number};

/// The feedback service's limit on the body, in UTF-16 units (`FEEDBACK_BODY_MAX_UNITS` in
/// server/src/feedback/draft.rs), less room for its footer.
const BODY_MAX_UNITS: usize = 19_000;
const TITLE_MAX_CHARS: usize = 110;
/// Longest note the person may add.
const NOTE_MAX_CHARS: usize = 2_000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CrashReport {
    pub(crate) title: String,
    pub(crate) body: String,
    /// Same crash, same id: the feedback service files every report with this id on one issue.
    pub(crate) fingerprint: String,
    /// `Removed: your home folder (now ~), 2 emails, 1 token`, when anything was.
    pub(crate) removed: Option<String>,
}

/// The crash's place in a few words: a source file and line, an exception name, or a
/// `module+offset`.
pub(crate) fn short_location(record: &CrashRecord) -> String {
    match record.kind {
        CrashKind::Panic => record
            .location
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&record.location)
            .to_string(),
        _ => record
            .location
            .split_whitespace()
            .next()
            .unwrap_or(&record.location)
            .to_string(),
    }
}

/// The issue for `record`, cleaned of personal data. `times` is how many times this crash
/// happened since the last report; `note` is what the person wrote, if anything.
pub(crate) fn build_report(
    record: &CrashRecord,
    times: usize,
    facts: &SystemFacts,
    note: &str,
    context: &FeedbackScrubContext,
) -> CrashReport {
    let fingerprint = record.fingerprint();
    let mut title: String = format!("Crash in {}: {}", record.version, short_location(record))
        .chars()
        .take(TITLE_MAX_CHARS)
        .collect();
    title = scrub_personal_data(&title, context);
    let note: String = note.trim().chars().take(NOTE_MAX_CHARS).collect();

    let mut stack = record.stack.clone();
    let mut log_tail = record.log_tail.clone();
    let mut left_out = (0usize, 0usize);
    loop {
        let raw = body_text(
            record,
            times,
            facts,
            &note,
            &fingerprint,
            &stack,
            &log_tail,
            left_out,
        );
        let body = scrub_personal_data(&raw, context);
        if body.encode_utf16().count() <= BODY_MAX_UNITS
            || (stack.is_empty() && log_tail.is_empty())
        {
            let removed = removed_summary(&raw, &body);
            return CrashReport {
                title,
                body,
                fingerprint,
                removed,
            };
        }
        // Too long for the service: the oldest log lines go first, then the deepest frames.
        if !log_tail.is_empty() {
            log_tail.remove(0);
            left_out.1 += 1;
        } else {
            stack.pop();
            left_out.0 += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn body_text(
    record: &CrashRecord,
    times: usize,
    facts: &SystemFacts,
    note: &str,
    fingerprint: &str,
    stack: &[String],
    log_tail: &[String],
    (frames_left_out, lines_left_out): (usize, usize),
) -> String {
    let recent_events = &record.recent_events;
    let what = match record.kind {
        CrashKind::Panic => "Ghostex closed unexpectedly (a Rust panic on the main thread).",
        CrashKind::UncaughtException => {
            "Ghostex closed unexpectedly (an Objective-C exception nothing caught)."
        }
        CrashKind::Native => "Ghostex was stopped by the system (a native crash).",
    };
    let mut ghostex = format!("Ghostex {}", record.version);
    if let Some(build) = build_number(&record.version) {
        ghostex.push_str(&format!(" ({build})"));
    }
    match &record.commit {
        Some(commit) if !commit.is_empty() => ghostex.push_str(&format!(" · commit {commit}")),
        _ => ghostex.push_str(" · commit unknown"),
    }
    ghostex.push_str(&format!(" · {}", facts.installed_as));
    if let Some(build_id) = &record.build_id {
        ghostex.push_str(&format!(" · build id {build_id}"));
    }
    let mut system = facts.os.clone();
    if let Some(model) = &facts.model {
        system.push_str(&format!(" · {model}"));
    }
    system.push_str(&format!(" · {}", facts.arch));
    let mut when = when_text(record.at_ms);
    if let Some(uptime) = record.uptime_ms {
        when.push_str(&format!(", {} after opening", duration_text(uptime)));
    }
    if times > 1 {
        when.push_str(&format!(", {times} times since the last report"));
    }

    let mut lines = vec![
        what.to_string(),
        String::new(),
        format!("- Crash: {}", record.location),
        format!("- Error: {}", one_line(&record.message)),
        format!("- {ghostex}"),
        format!("- System: {system}"),
        format!("- When: {when}"),
        format!("- Crash id: {fingerprint}"),
    ];
    if let Some(thread) = &record.thread {
        lines.push(format!("- Thread: {thread}"));
    }
    if let Some(screen) = &record.screen {
        lines.push(format!("- On screen: {screen}"));
    }
    if !note.is_empty() {
        lines.push(String::new());
        lines.push(format!("What they were doing: {}", one_line(note)));
    }
    if !stack.is_empty() || frames_left_out > 0 {
        lines.push(String::new());
        lines.push("Stack".to_string());
        lines.push("```text".to_string());
        lines.extend(stack.iter().map(|frame| fenced(frame)));
        if frames_left_out > 0 {
            lines.push(format!("… {frames_left_out} more frames left out to fit"));
        }
        lines.push("```".to_string());
    }
    if !recent_events.is_empty() {
        lines.push(String::new());
        lines.push("Last app events (UTC)".to_string());
        lines.push("```text".to_string());
        lines.extend(recent_events.iter().map(|event| fenced(event)));
        lines.push("```".to_string());
    }
    if !log_tail.is_empty() || lines_left_out > 0 {
        lines.push(String::new());
        lines.push(format!("Last {} log lines", log_tail.len()));
        lines.push("```text".to_string());
        if lines_left_out > 0 {
            lines.push(format!("… {lines_left_out} older lines left out to fit"));
        }
        lines.extend(log_tail.iter().map(|line| fenced(line)));
        lines.push("```".to_string());
    }
    lines.join("\n")
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A line inside a ```text block can't close it.
fn fenced(line: &str) -> String {
    line.replace("```", "'''")
}

fn when_text(at_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms as i64)
        .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn duration_text(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds < 120 {
        format!("{} s", seconds.max(1))
    } else if seconds < 7200 {
        format!("{} min", seconds / 60)
    } else {
        format!("{} h", seconds / 3600)
    }
}

/// What the cleaning replaced, counted from the placeholders it left.
fn removed_summary(raw: &str, cleaned: &str) -> Option<String> {
    let added = |placeholder: &str| {
        cleaned
            .matches(placeholder)
            .count()
            .saturating_sub(raw.matches(placeholder).count())
    };
    let plural = |count: usize, one: &str, many: &str| {
        if count == 1 {
            format!("1 {one}")
        } else {
            format!("{count} {many}")
        }
    };
    let mut parts = Vec::new();
    if added("~") > 0 {
        parts.push("your home folder (now ~)".to_string());
    }
    let emails = added("<email>");
    if emails > 0 {
        parts.push(plural(emails, "email", "emails"));
    }
    let tokens = added("<token>") + added("<redacted>") + added("<private key>");
    if tokens > 0 {
        parts.push(plural(tokens, "token or key", "tokens or keys"));
    }
    let names = added("<private>") + added("<user>");
    if names > 0 {
        parts.push(plural(names, "name", "names"));
    }
    let addresses = added("<host>") + added("<ip>");
    if addresses > 0 {
        parts.push(plural(addresses, "address", "addresses"));
    }
    // Paths in the error and the log were already replaced when the crash was saved.
    let paths = raw.matches("[path]").count();
    if paths > 0 {
        parts.push(plural(paths, "file path", "file paths"));
    }
    (!parts.is_empty()).then(|| format!("Removed: {}", parts.join(", ")))
}

/// What happened, in one line, for the crash window:
/// `Ghostex 11.8.2 crashed in list.rs:488: RefCell already mutably borrowed`.
pub(crate) fn summary_line(record: &CrashRecord, context: &FeedbackScrubContext) -> String {
    const MAX_CHARS: usize = 150;
    let mut line = format!(
        "Ghostex {} crashed in {}",
        record.version,
        short_location(record)
    );
    let message = one_line(&record.message);
    if !message.is_empty() {
        line.push_str(": ");
        line.push_str(&message);
    }
    let mut line = scrub_personal_data(&line, context);
    if line.chars().count() > MAX_CHARS {
        line = line.chars().take(MAX_CHARS - 1).collect();
        line.push('…');
    }
    line
}
