//! The title a row draws and the tooltip it shows, plus its two clock labels.
//!
//! SEE-ALSO: packages/core-ui/session-card-presentation.ts (deleted 2026-10-01) and packages/core-ui/relative-time.ts (deleted 2026-10-01).

use super::agents::tooltip_strip_labels;
use super::inputs::CloseAfterDoneInput;
use super::tags::{tag_label, TagCatalog};
use super::text::{
    is_js_line_terminator, js_trim, js_trim_start, normalized_non_empty, parse_iso_ms, utf16_len,
    utf16_prefix, utf16_suffix,
};
use super::view::{LabelDeadline, SessionRow};

const DEFAULT_TERMINAL_SESSION_TITLE: &str = "Terminal Session";
const TERMINAL_TITLE_MARKER: &str = "∗";
const UNSYNCED_TITLE_LABEL: &str = "(Unsynced title)";
const SESSION_NOTE_TOOLTIP_MAX_UTF16: usize = 400;

/// Everything the title and tooltip rules read from a row.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TitleInput<'a> {
    pub(crate) agent_icon: Option<&'a str>,
    pub(crate) alias: &'a str,
    pub(crate) display_title: Option<&'a str>,
    pub(crate) display_title_tooltip: Option<&'a str>,
    pub(crate) is_browser: bool,
    pub(crate) is_primary_title_terminal_title: bool,
    pub(crate) primary_title: Option<&'a str>,
    pub(crate) terminal_title: Option<&'a str>,
    pub(crate) detail: Option<&'a str>,
    pub(crate) session_note: Option<&'a str>,
    pub(crate) pending_question_count: u64,
    pub(crate) awaiting_approval: bool,
    pub(crate) activity: &'a str,
    pub(crate) delayed_send_remaining_label: Option<&'a str>,
    pub(crate) close_after_done: Option<&'a CloseAfterDoneInput>,
    pub(crate) effective_tag: Option<&'a str>,
    pub(crate) is_live: Option<bool>,
    pub(crate) is_running: Option<bool>,
    pub(crate) is_sleeping: Option<bool>,
    pub(crate) lifecycle_state: Option<&'a str>,
    pub(crate) native_pane_state: Option<&'a str>,
    pub(crate) provider_session_state: Option<&'a str>,
    pub(crate) session_persistence_provider: Option<&'a str>,
    pub(crate) session_persistence_name: Option<&'a str>,
    pub(crate) session_routing_id: Option<&'a str>,
    pub(crate) session_number: Option<&'a str>,
    pub(crate) agent_session_id: Option<&'a str>,
}

struct HeadingTitle {
    is_ghost_placeholder: bool,
    text: Option<String>,
}

/// `normalizeSessionCardHeadingTitle`.
fn normalize_heading_title(title: Option<&str>) -> HeadingTitle {
    let Some(normalized) = normalized_non_empty(title) else {
        return HeadingTitle {
            is_ghost_placeholder: false,
            text: None,
        };
    };
    // `/^👻(?:\s+Terminal Session)?$/u` against the whitespace-collapsed title.
    if normalized == "👻" || normalized == "👻 Terminal Session" {
        return HeadingTitle {
            is_ghost_placeholder: true,
            text: Some(DEFAULT_TERMINAL_SESSION_TITLE.to_string()),
        };
    }
    HeadingTitle {
        is_ghost_placeholder: false,
        text: Some(normalized),
    }
}

fn non_persistent_heading(heading: &str, include_unsynced: bool) -> String {
    if include_unsynced {
        format!("{TERMINAL_TITLE_MARKER} {heading} {UNSYNCED_TITLE_LABEL}")
    } else {
        format!("{TERMINAL_TITLE_MARKER} {heading}")
    }
}

/// `formatSessionHeadingText`.
pub(crate) fn session_heading(input: &TitleInput<'_>, include_unsynced: bool) -> String {
    if let Some(display_title) = normalized_non_empty(input.display_title) {
        return if include_unsynced {
            normalized_non_empty(input.display_title_tooltip).unwrap_or(display_title)
        } else {
            display_title
        };
    }
    let primary = normalize_heading_title(input.primary_title);
    let terminal = normalize_heading_title(input.terminal_title);
    let primary_text = primary.text.clone();
    let base = if primary.text.is_some() {
        primary
    } else {
        normalize_heading_title(Some(input.alias))
    };
    let base_text = base
        .text
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| input.alias.to_string());
    if base.is_ghost_placeholder {
        return non_persistent_heading(&base_text, include_unsynced);
    }
    if input.is_browser
        || input.is_primary_title_terminal_title
        || primary_text.is_none()
        || primary_text == terminal.text
    {
        return base_text;
    }
    non_persistent_heading(&base_text, include_unsynced)
}

/// `getSessionCardTitleTooltip(...).tooltip`, which the native sidebar always shows on hover.
pub(crate) fn session_tooltip(
    input: &TitleInput<'_>,
    catalog: &TagCatalog,
    show_debug_session_numbers: bool,
    always_show_state_tooltip: bool,
) -> String {
    let tooltip_heading = session_heading(input, true);
    let heading = match tag_label(input.effective_tag, catalog) {
        Some(label) => format!("[{label}] {tooltip_heading}"),
        None => tooltip_heading,
    };
    let session_id_tooltip = show_debug_session_numbers
        .then(|| {
            let value = js_trim(input.session_routing_id.unwrap_or(""));
            if !value.is_empty() {
                Some(value.to_string())
            } else {
                let number = js_trim(input.session_number.unwrap_or(""));
                (!number.is_empty()).then(|| number.to_string())
            }
        })
        .flatten()
        .map(|value| format!("ID: {value}"));

    let mut lines: Vec<String> = Vec::new();
    if input.awaiting_approval {
        lines.push("Waiting for your approval".to_string());
    }
    if input.pending_question_count > 0 {
        let prefix = if input.activity == "working" {
            "Working · "
        } else {
            ""
        };
        lines.push(format!(
            "{prefix}Answer requested ({})",
            input.pending_question_count
        ));
    }
    if let Some(label) = input
        .delayed_send_remaining_label
        .filter(|label| !label.is_empty())
    {
        lines.push(delayed_send_tooltip_text(label));
    }
    if let Some(close) = input.close_after_done {
        match close
            .remaining_label
            .as_deref()
            .filter(|label| !label.is_empty())
        {
            Some(label) => lines.push(format!("Close After Done in {label}")),
            None if close.armed => lines.push("Close After Done armed".to_string()),
            None => {}
        }
    }
    if let Some(note) = input
        .session_note
        .map(js_trim)
        .filter(|note| !note.is_empty())
    {
        let bounded = if utf16_len(note) > SESSION_NOTE_TOOLTIP_MAX_UTF16 {
            format!("{}…", utf16_prefix(note, SESSION_NOTE_TOOLTIP_MAX_UTF16))
        } else {
            note.to_string()
        };
        lines.push(format!("Note: {bounded}"));
    }
    if show_debug_session_numbers || always_show_state_tooltip {
        if let Some(label) = session_state_tooltip_label(input) {
            lines.push(format!("State: {label}"));
        }
    }
    if let Some(secondary) = session_tooltip_secondary_text(input) {
        lines.push(secondary);
    }
    if show_debug_session_numbers {
        if let Some(agent_session_id) = input
            .agent_session_id
            .map(js_trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(agent_session_id.to_string());
        }
    }
    if session_id_tooltip.is_none() && show_debug_session_numbers {
        if let (Some(name), Some(provider)) = (
            input
                .session_persistence_name
                .filter(|value| !value.is_empty()),
            input
                .session_persistence_provider
                .filter(|value| !value.is_empty()),
        ) {
            lines.push(format!("{provider} session: {name}"));
        }
    }

    build_title_tooltip(&heading, &lines.join("\n"), session_id_tooltip.as_deref())
}

/// `getDelayedSendTooltipText`.
fn delayed_send_tooltip_text(remaining_label: &str) -> String {
    match remaining_label {
        "Waiting for agent" => {
            "Delayed Send: the prompt will be sent when the agent finishes working".to_string()
        }
        "Waiting for agents" => {
            "Delayed Send: the prompt will be sent when all agents finish working".to_string()
        }
        label => format!("Delayed Send: the prompt will be sent in {label}"),
    }
}

/// `buildSessionTitleTooltip`: one blank line between blocks, every line trimmed, repeats dropped.
fn build_title_tooltip(heading: &str, secondary: &str, session_id_tooltip: Option<&str>) -> String {
    let mut unique: Vec<String> = Vec::new();
    for block in [Some(heading), Some(secondary), session_id_tooltip]
        .into_iter()
        .flatten()
    {
        for line in split_js_lines(block) {
            let line = js_trim(line);
            if line.is_empty() || unique.iter().any(|seen| seen == line) {
                continue;
            }
            unique.push(line.to_string());
        }
    }
    unique.join("\n\n")
}

/// `value.split(/\r?\n/u)`.
fn split_js_lines(value: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            let end = if index > 0 && bytes[index - 1] == b'\r' {
                index - 1
            } else {
                index
            };
            lines.push(&value[start..end]);
            start = index + 1;
        }
        index += 1;
    }
    lines.push(&value[start..]);
    lines
}

/// `getSessionTooltipSecondaryText`.
fn session_tooltip_secondary_text(input: &TitleInput<'_>) -> Option<String> {
    if let Some(detail) = strip_agent_tooltip_text(input.detail, input.agent_icon) {
        if !contains_filesystem_path(&detail) {
            return Some(detail);
        }
    }
    let terminal = normalize_heading_title(input.terminal_title);
    if !terminal.is_ghost_placeholder {
        if let Some(terminal_title) =
            strip_agent_tooltip_text(terminal.text.as_deref(), input.agent_icon)
        {
            if !contains_filesystem_path(&terminal_title) {
                return Some(terminal_title);
            }
        }
    }
    // `activityLabel` is never set on a gxserver row.
    None
}

/// `stripAgentTooltipText`.
fn strip_agent_tooltip_text(value: Option<&str>, agent_icon: Option<&str>) -> Option<String> {
    let normalized = js_trim(value?);
    if normalized.is_empty() {
        return None;
    }
    let Some(agent_icon) = agent_icon else {
        return Some(normalized.to_string());
    };
    let lower_value = normalized.to_lowercase();
    for label in tooltip_strip_labels(agent_icon) {
        let lower_label = label.to_lowercase();
        if lower_value == lower_label {
            return None;
        }
        if !lower_value.starts_with(&lower_label) {
            continue;
        }
        let remainder = js_trim_start(utf16_suffix(normalized, utf16_len(label)));
        if remainder.is_empty() {
            return None;
        }
        // `/^([:/|-]+)\s*(.*)$/`: `.` does not match a line terminator and `$` is the end of the
        // whole string, so a remainder that wraps lines does not match.
        let separators = remainder
            .chars()
            .take_while(|character| matches!(character, ':' | '/' | '|' | '-'))
            .count();
        if separators == 0 {
            return Some(normalized.to_string());
        }
        let after_separators: String = remainder.chars().skip(separators).collect();
        let rest = js_trim_start(&after_separators);
        if rest.chars().any(is_js_line_terminator) {
            return Some(normalized.to_string());
        }
        let stripped = js_trim(rest);
        return (!stripped.is_empty()).then(|| stripped.to_string());
    }
    Some(normalized.to_string())
}

/// `FILESYSTEM_PATH_TOOLTIP_PATTERN`. Every candidate is a slice of the value, so a long tooltip
/// line costs no allocation.
fn contains_filesystem_path(value: &str) -> bool {
    const ROOTS: [&str; 14] = [
        "~/",
        "file://",
        "/Applications/",
        "/Library/",
        "/System/",
        "/Users/",
        "/Volumes/",
        "/etc/",
        "/home/",
        "/opt/",
        "/private/",
        "/tmp/",
        "/usr/",
        "/var/",
    ];
    let mut previous: Option<char> = None;
    for (index, character) in value.char_indices() {
        let at_boundary = previous.is_none_or(super::text::is_js_whitespace);
        previous = Some(character);
        if !at_boundary {
            continue;
        }
        let rest = &value[index..];
        if ROOTS.iter().any(|root| rest.starts_with(root)) {
            return true;
        }
        // `[A-Za-z]:[\\/]`
        let mut drive = rest.chars();
        if drive
            .next()
            .is_some_and(|letter| letter.is_ascii_alphabetic())
            && drive.next() == Some(':')
            && matches!(drive.next(), Some('\\') | Some('/'))
        {
            return true;
        }
    }
    false
}

/// `getSessionStateTooltipLabel`.
fn session_state_tooltip_label(input: &TitleInput<'_>) -> Option<&'static str> {
    let has_state_signal = input.is_live.is_some()
        || input.is_running.is_some()
        || input.is_sleeping.is_some()
        || input.lifecycle_state.is_some()
        || input.native_pane_state.is_some()
        || input.provider_session_state.is_some();
    if !has_state_signal {
        return None;
    }
    if matches!(input.native_pane_state, Some("mounted") | Some("mounting")) {
        return Some("Active in app");
    }
    if input.provider_session_state == Some("exists") {
        return Some("Active, not loaded");
    }
    if input.is_sleeping == Some(true) || input.lifecycle_state == Some("sleeping") {
        return Some("Sleeping");
    }
    if input.provider_session_state == Some("unknown") || input.lifecycle_state == Some("error") {
        return Some("Unknown");
    }
    if input.is_live == Some(true)
        || input.is_running == Some(true)
        || input.lifecycle_state == Some("running")
    {
        return Some("Active in app");
    }
    if input.provider_session_state == Some("missing")
        && input.session_persistence_provider.is_some()
    {
        return Some("Not started");
    }
    if input.lifecycle_state == Some("done")
        || input.is_running == Some(false)
        || input.is_live == Some(false)
    {
        return Some("Done");
    }
    None
}

/// `getSessionCardTimerTrailingLabel`.
///
/// CDXC:Sessions 2026-09-27 DECISION:
/// User: an armed Close After Done on a session that is still running must not show a time; the time appears only once the countdown to close starts. The row used to draw a standing `03:00` while it waited for the agent to finish, which read as the time left; the clock icon and the tooltip ("Close After Done armed") already say it is armed.
pub(crate) fn timer_trailing_label(row: &SessionRow, now_ms: u64) -> Option<String> {
    if let Some(delayed) = &row.delayed_send {
        if let Some(deadline) = delayed
            .deadline_at
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            return deadline_countdown(deadline, now_ms);
        }
        if let Some(label) = delayed
            .remaining_label
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            return (label != "Waiting for agent" && label != "Waiting for agents")
                .then(|| label.to_string());
        }
    }
    let close = row.close_after_done.as_ref()?;
    if let Some(deadline) = close
        .deadline_at
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        return deadline_countdown(deadline, now_ms);
    }
    close
        .remaining_label
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn deadline_countdown(deadline_at: &str, now_ms: u64) -> Option<String> {
    let deadline = parse_iso_ms(deadline_at)?;
    Some(format_timer_countdown(deadline - now_ms as i64))
}

/// `formatSessionTimerCountdown`.
fn format_timer_countdown(delay_ms: i64) -> String {
    let total_seconds = (delay_ms as f64 / 1000.0).ceil().max(0.0) as i64;
    let hours = total_seconds / 3600;
    let minutes = total_seconds % 3600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// `formatRelativeTime(at, { allowJustNow: false }).value`.
pub(crate) fn last_interaction_label(at: &str, now_ms: u64) -> String {
    let Some(at_ms) = parse_iso_ms(at) else {
        // `new Date(invalid).getTime()` is NaN, and every comparison below it is false.
        return "NaNd".to_string();
    };
    let difference = (now_ms as i64 - at_ms).max(0);
    let seconds = difference / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

/// The next host time at which the time a row draws reads differently.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// The labels are the one part of a row the host formats against its own clock, so nothing in the
/// store ever tells it they moved. The projection this port replaces refreshed them from a timer
/// that ran once a second for the life of the app; this says exactly when the next one changes, so
/// a countdown still ticks every second while it runs and an idle sidebar with an hour-old row
/// wakes once an hour instead of three thousand six hundred times. A row draws one time and only
/// one: a live countdown, else a standing timer label that never moves, else the relative time,
/// and only when the card shows it at all. Asking about the two the row does not draw is how a
/// sidebar full of working sessions ends up waking every second for labels nobody sees.
pub(crate) fn next_label_deadline(
    row: &SessionRow,
    now_ms: u64,
    show_relative_time: bool,
) -> Option<LabelDeadline> {
    let now = now_ms as i64;
    let countdown = [
        row.delayed_send
            .as_ref()
            .and_then(|delayed| delayed.deadline_at.as_deref()),
        row.close_after_done
            .as_ref()
            .and_then(|close| close.deadline_at.as_deref()),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.is_empty())
    .filter_map(parse_iso_ms)
    // A countdown that has run out reads `00:00` and stands still.
    .filter(|deadline| *deadline > now)
    // The seconds the countdown shows are rounded up, so it changes on the second boundaries
    // measured back from the deadline.
    .map(|deadline| deadline - ((deadline - now - 1) / 1000) * 1000)
    .min();
    if let Some(countdown) = countdown {
        return Some(LabelDeadline::Countdown(countdown.max(now + 1) as u64));
    }
    // A Delayed Send with no deadline still owns the slot, with a label that says what it is
    // waiting for rather than a time.
    if timer_trailing_label(row, now_ms).is_some() {
        return None;
    }
    // The relative time is drawn only when the card shows it and the row is neither working nor
    // waiting for an answer nor running background work, which are drawn as their own state instead.
    if !show_relative_time
        || row.activity == "working"
        || row.activity == "attention"
        || row.has_background_work
    {
        return None;
    }
    let at = parse_iso_ms(row.last_interaction_at.as_deref()?)?;
    let elapsed = (now - at).max(0);
    let step = match elapsed {
        ..60_000 => 1_000,
        60_000..3_600_000 => 60_000,
        3_600_000..86_400_000 => 3_600_000,
        _ => 86_400_000,
    };
    let deadline = at + (elapsed / step + 1) * step;
    (deadline > now).then_some(LabelDeadline::Relative(deadline as u64))
}
