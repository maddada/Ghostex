use super::*;

/// `1h 2m 3s` / `1m 1s` / `45s` → seconds. `None` unless EVERY token parsed,
/// so a half-read clock is dropped rather than shown wrong.
pub(crate) fn parse_elapsed_seconds(text: &str) -> Option<u64> {
    let mut total: u64 = 0;
    let mut matched = false;
    for token in text.split_whitespace() {
        let (digits, unit) = token.split_at(token.find(|ch: char| !ch.is_ascii_digit())?);
        let value: u64 = digits.parse().ok()?;
        total += match unit {
            "h" => value * 3_600,
            "m" => value * 60,
            "s" => value,
            _ => return None,
        };
        matched = true;
    }
    matched.then_some(total)
}

/// The `(1m 1s)` a spinner line trails, if it has one.
fn trailing_parenthetical(line: &str) -> Option<&str> {
    let close = line.rfind(')')?;
    let open = line[..close].rfind('(')?;
    Some(line[open + 1..close].trim())
}

/// Claude appends ` · 22s` to a running tool row and repaints only the clock.
/// Keep that clock as progress metadata so one tool run does not become a new
/// transient chat message every second.
pub(super) fn trailing_elapsed_status(label: &str) -> (&str, Option<u64>) {
    let Some((stable_label, elapsed)) = label.rsplit_once(" · ") else {
        return (label, None);
    };
    let Some(elapsed_seconds) = parse_elapsed_seconds(elapsed.trim()) else {
        return (label, None);
    };
    (stable_label.trim_end(), Some(elapsed_seconds))
}

/// Claude's animated working line can repaint the same wording as:
///
/// ```text
/// label… (1m 5s · thinking with medium effort)
/// label… (1m 11s · ↓ 4.5k tokens)
/// ```
///
/// The parenthetical is sample metadata, not a new status. Separating its
/// clock keeps one stable label, which lets the client's exact-text
/// deduplication retain one row instead of one row per screen probe.
fn trailing_parenthetical_status(label: &str) -> (&str, Option<u64>) {
    let Some(without_close) = label.strip_suffix(')') else {
        return (label, None);
    };
    let Some(open) = without_close.rfind(" (") else {
        return (label, None);
    };
    let metadata = &without_close[open + 2..];
    let elapsed = metadata
        .split_once(" · ")
        .map_or(metadata, |(elapsed, _)| elapsed);
    let Some(elapsed_seconds) = parse_elapsed_seconds(elapsed.trim()) else {
        return (label, None);
    };
    (without_close[..open].trim_end(), Some(elapsed_seconds))
}

fn stable_status_label(label: &str) -> (&str, Option<u64>) {
    let (stable, elapsed_seconds) = trailing_elapsed_status(label);
    if elapsed_seconds.is_some() {
        return (stable, elapsed_seconds);
    }
    trailing_parenthetical_status(label)
}

/// The one currently understood status that Claude paints with a non-`⏺`
/// marker. Match the whole grammar so arbitrary/custom spinner text cannot be
/// admitted merely because it happens to use the same animated glyph.
fn is_dynamic_workflow_wait_label(label: &str) -> bool {
    let Some(rest) = label.strip_prefix("Waiting for ") else {
        return false;
    };
    let Some((count, suffix)) = rest.split_once(' ') else {
        return false;
    };
    let Ok(count) = count.parse::<u64>() else {
        return false;
    };
    (count == 1 && suffix == "dynamic workflow to finish")
        || (count > 1 && suffix == "dynamic workflows to finish")
}

/// Claude picks a playful action word for this row (`Cooked`, `Crunched`,
/// `Sautéed`, ...), so the stable evidence is the rest of its whole grammar:
///
/// ```text
/// <one alphabetic word> for <duration> · <count> shell(s) still running
/// <one alphabetic word> for <duration> · done <time> · <count> shell(s) still running
/// ```
///
/// Claude paints the identical grammar for a running Monitor
/// (`… · 1 monitor still running`), so the unit word may also be
/// `monitor`/`monitors`; both share the `shells-running` activity kind.
///
/// The first shape is an advancing clock, so move it into progress metadata.
/// In the second shape Claude has frozen that duration and added a completion
/// timestamp; keep both in the stable label rather than making a finished
/// duration tick forward in the client.
fn running_shells_activity(label: &str) -> Option<SessionChatTerminalActivity> {
    let (action, rest) = label.split_once(" for ")?;
    if action.is_empty() || !action.chars().all(char::is_alphabetic) {
        return None;
    }
    let (timing, shell_status) = rest.rsplit_once(" · ")?;
    let (elapsed, completed_at) = match timing.split_once(" · ") {
        Some((elapsed, completed_at)) if completed_at.trim().starts_with("done ") => {
            (elapsed, Some(completed_at.trim()))
        }
        Some(_) => return None,
        None => (timing, None),
    };
    let elapsed_seconds = parse_elapsed_seconds(elapsed.trim())?;
    let (count, suffix) = shell_status.trim().split_once(' ')?;
    let count = count.parse::<u64>().ok()?;
    let valid_suffix = if count == 1 {
        suffix == "shell still running" || suffix == "monitor still running"
    } else {
        suffix == "shells still running" || suffix == "monitors still running"
    };
    if !valid_suffix {
        return None;
    }

    let background = SessionChatBackgroundWork {
        count,
        tasks: Vec::new(),
    };
    let mut activity = if let Some(completed_at) = completed_at {
        SessionChatTerminalActivity::new(
            SESSION_CHAT_ACTIVITY_SHELLS_RUNNING,
            format!(
                "{action} for {} · {completed_at} · {count} {suffix}",
                elapsed.trim()
            ),
        )
    } else {
        let mut activity = SessionChatTerminalActivity::new(
            SESSION_CHAT_ACTIVITY_SHELLS_RUNNING,
            format!("{action} · {count} {suffix}"),
        );
        activity.elapsed_seconds = Some(elapsed_seconds);
        activity
    };
    activity.background = Some(background);
    Some(activity)
}

/// `49%` anywhere on the line (the bar glyphs around it are ignored).
pub(super) fn parse_percent(line: &str) -> Option<u8> {
    for token in line.split_whitespace() {
        let Some(digits) = token.strip_suffix('%') else {
            continue;
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if let Ok(percent) = digits.parse::<u8>() {
            if percent <= 100 {
                return Some(percent);
            }
        }
    }
    None
}

/*
A label only counts when the line is the CLI's own status row rather than prose
that mentions it. Compaction requires decoration-only text before its phrase;
general status requires `⏺`, and another star marker requires an allowlisted
whole label. An assistant sentence, a tip, and custom spinner wording cannot
satisfy those shapes.
*/
pub(super) fn compacting_activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
    if let Some(at) = line.find(COMPACTING_LABEL) {
        if !line[..at]
            .chars()
            .any(|ch| ch.is_alphabetic() || ch.is_ascii_digit())
        {
            let mut activity = SessionChatTerminalActivity::new(
                SESSION_CHAT_ACTIVITY_COMPACTING,
                "Compacting conversation",
            );
            /*
            CDXC:AgentScreenDetection 2026-09-29 DECISION:
            User: the chat's compaction card must show the same details as the terminal row
            (`✢ Compacting conversation… (17s · ↓ 901 tokens)`). Claude now appends the token
            counter after the clock, and parsing the whole parenthetical as a duration dropped
            the clock too, so the card showed neither.
            */
            if let Some(metadata) = trailing_parenthetical(line) {
                let mut parts = metadata.split(" · ");
                activity.elapsed_seconds = parts.next().and_then(parse_elapsed_seconds);
                activity.tokens = parts
                    .map(str::trim)
                    .find(|part| part.ends_with(" tokens"))
                    .map(str::to_owned);
            }
            return Some(activity);
        }
    }

    None
}

/// CDXC:AgentScreenDetection 2026-09-13 DECISION:
/// User: Codex compaction uses Claude's status card, with a looping bar because Codex reports no percentage.
/// Its live status row also holds the prompt queue through the shared compaction marker.
pub(super) fn codex_compacting_activity_from_line(
    line: &str,
) -> Option<SessionChatTerminalActivity> {
    // CDXC:AgentScreenDetection 2026-09-15 WHY: Codex's live status starts in column zero; quoted status in user messages, drafts and tool output is indented, so trimming the gutter kept chat stuck compacting on a pasted terminal capture.
    // Codex appends background-terminal and hook status after the closing parenthesis, so requiring the clock to end the line also hid active compaction from chat.
    let status = line.trim_end().strip_prefix('•')?.trim_start();
    let (metadata, suffix) = status
        .strip_prefix("Compacting context (")?
        .split_once(')')?;
    if !suffix.is_empty() && !suffix.starts_with(" · ") {
        return None;
    }
    let (elapsed, interrupt) = metadata.split_once('•')?;
    if interrupt.trim() != "esc to interrupt" {
        return None;
    }
    let mut activity = SessionChatTerminalActivity::new(
        SESSION_CHAT_ACTIVITY_COMPACTING,
        "Compacting conversation",
    );
    activity.elapsed_seconds = Some(parse_elapsed_seconds(elapsed.trim())?);
    Some(activity)
}

pub(super) fn activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
    if let Some(activity) = compacting_activity_from_line(line) {
        return Some(activity);
    }

    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '⏺' && !CLAUDE_SPECIAL_STATUS_MARKERS.contains(marker) {
        return None;
    }
    let rest = &trimmed[marker.len_utf8()..];
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    claude_status_from_label(marker, rest.trim())
}

/// The activity a Claude status row carries once its marker is known.
pub(super) fn claude_status_from_label(
    marker: char,
    raw_label: &str,
) -> Option<SessionChatTerminalActivity> {
    if let Some(activity) = running_shells_activity(raw_label) {
        return Some(activity);
    }
    let (label, elapsed_seconds) = stable_status_label(raw_label);
    if label.is_empty() {
        return None;
    }
    if marker != '⏺' && !is_dynamic_workflow_wait_label(label) {
        return None;
    }
    let mut activity = SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_CLAUDE_STATUS, label);
    activity.elapsed_seconds = elapsed_seconds;
    Some(activity)
}

/*
Cursor's working row is a Braille spinner followed by one owned state and an
optional token counter:

    ⠠⠜ Thinking 73 tokens
    ⠋ Composing 1.2K tokens
    ⠠⠜ Summarizing 42.61k tokens

The spinner is required so assistant prose containing these labels cannot be
mistaken for live activity. The token count is intentionally not projected:
it is throughput metadata, not stable reasoning content.
*/
/// CDXC:AgentScreenDetection 2026-09-16 DECISION:
/// User: Cursor's /summarize and /compact use the same chat compaction flow as Codex, detected from its live Summarizing spinner row.
/// Cursor reports tokens, not completion percentage, so the shared compaction card uses a looping bar and holds queued prompts until summarizing ends.
pub(super) fn cursor_activity_from_line(line: &str) -> Option<SessionChatTerminalActivity> {
    let mut tokens = line.split_whitespace();
    let spinner = tokens.next()?;
    if spinner.is_empty()
        || !spinner
            .chars()
            .all(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
    {
        return None;
    }
    let label = tokens.next()?;
    if !matches!(label, "Thinking" | "Composing" | "Summarizing") {
        return None;
    }
    let remaining: Vec<_> = tokens.collect();
    if !remaining.is_empty()
        && (remaining.len() != 2
            || remaining[1] != "tokens"
            || !remaining[0]
                .trim_end_matches(['k', 'K', 'm', 'M'])
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == '.'))
    {
        return None;
    }
    let (kind, label) = if label == "Summarizing" {
        (SESSION_CHAT_ACTIVITY_COMPACTING, "Compacting conversation")
    } else {
        (SESSION_CHAT_ACTIVITY_CURSOR_THINKING, label)
    };
    Some(SessionChatTerminalActivity::new(kind, label))
}

/// CDXC:AgentScreenDetection 2026-09-16 DECISION:
/// User: Grok Build's /compact uses the same chat compaction flow as Cursor and Codex, based on its actual terminal status.
/// The live Braille Compacting… row ends in [stop] immediately above the composer; the transcript's Compacting conversation… and completed notice remain after it finishes.
pub(super) fn grok_compacting_activity(screen_text: &str) -> Option<SessionChatTerminalActivity> {
    let lines = crate::session_chat_agent_fleet::normalized_screen_lines(screen_text);
    let composer = lines.iter().rposition(|line| line.starts_with("│ ❯"))?;
    let border = lines.get(composer.checked_sub(1)?)?;
    if !border.starts_with("╭─") || !border.ends_with('╮') {
        return None;
    }
    let line = lines.get(composer.checked_sub(2)?)?;
    let mut tokens = line.split_whitespace();
    let spinner = tokens.next()?;
    if !spinner
        .chars()
        .all(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
        || tokens.next()? != "Compacting…"
        || !line.ends_with(" [stop]")
    {
        return None;
    }
    let mut activity =
        SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_COMPACTING, COMPACTING_LABEL);
    activity.elapsed_seconds = tokens
        .next()
        .and_then(|clock| clock.strip_suffix('s'))
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| seconds as u64);
    Some(activity)
}

/// CDXC:AgentScreenDetection 2026-09-28 DECISION:
/// User: a Hermes compress shows the shared compaction card while it runs, then one line in chat: "Context compacted" or "Nothing to compress".
/// `/compress` prints `⏳ Compressing context...` when it starts and Hermes keeps its "command in progress" hint directly above the status bar until it ends. Stale copies of both stay in scrollback, so only the hint in the bottom layout counts.
pub(super) fn hermes_compacting_activity(screen_text: &str) -> Option<SessionChatTerminalActivity> {
    let lines = crate::session_chat_agent_fleet::normalized_screen_lines(screen_text);
    let status_bar = lines
        .iter()
        .rposition(|line| crate::session_chat_options::is_hermes_statusline(line))?;
    if !crate::session_chat_options::is_hermes_busy_hint(lines.get(status_bar.checked_sub(1)?)?) {
        return None;
    }
    let started = lines[..status_bar]
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("⏳ "))?;
    (started == "Compressing context...").then(|| {
        SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_COMPACTING, COMPACTING_LABEL)
    })
}
