//! `isDefaultSessionSearchTitle` and the terminal-title normalization it depends on
//! (packages/shared/session-grid-contract-session.ts): a creation default such as
//! `Codex Session`, a numbered `Session 3`, a path, or an agent status word is not a name a
//! session search should find.
//!
//! CDXC:Sessions 2026-09-30 SEE-ALSO:
//! apps/mobile/app/src/sessions/sessionSearch.ts keeps a copy of these placeholder titles for the phone's session search; add a new agent's default title there too.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::sidebar_view::text::{collapse_js_whitespace, js_trim};

const DEFAULT_TERMINAL_SESSION_TITLE: &str = "Terminal Session";

const IGNORED_PLACEHOLDER_SESSION_TITLES: &[&str] = &[
    "terminal session",
    "amp cli session",
    "amp session",
    "antigravity cli session",
    "antigravity session",
    "claude session",
    "claude code session",
    "codebuddy session",
    "code buddy session",
    "codex session",
    "codex cli session",
    "command code session",
    "commandcode session",
    "copilot session",
    "cursor agent session",
    "cursor cli session",
    "cursor session",
    "freebuff session",
    "deepseek harness session",
    "dsh session",
    "mastra session",
    "mastra code session",
    "devin session",
    "droid session",
    "factory droid session",
    "gemini session",
    "grok session",
    "grok build session",
    "hermes session",
    "hermes agent session",
    "kimi session",
    "kimi code session",
    "kiro session",
    "kiro cli session",
    "omp session",
    "openclaude session",
    "open claude session",
    "opencode session",
    "open code session",
    "openai codex session",
    "pi session",
    "qoder session",
    "qodercli session",
    "rovo session",
    "rovo dev session",
    "rovodev session",
];

/// The distinct values of `DEFAULT_SESSION_AGENT_TITLE_NAMES`, in first-seen order.
const DEFAULT_SESSION_AGENT_TITLE_NAMES: &[&str] = &[
    "Antigravity CLI",
    "Amp CLI",
    "Claude",
    "CodeBuddy",
    "Codex",
    "Command Code",
    "Copilot",
    "Cursor CLI",
    "Mastra Code",
    "Devin",
    "Factory Droid",
    "Empryo",
    "Freebuff",
    "DeepSeek Harness",
    "Gemini",
    "Grok Build",
    "Hermes Agent",
    "Kimi Code",
    "Kiro CLI",
    "OMP",
    "OpenClaude",
    "OpenCode",
    "Pi",
    "Qoder",
    "Rovo Dev",
];

/// `createDefaultSessionSearchPlaceholderTitles`.
fn placeholder_titles() -> &'static BTreeSet<String> {
    static TITLES: OnceLock<BTreeSet<String>> = OnceLock::new();
    TITLES.get_or_init(|| {
        let mut titles: BTreeSet<String> = IGNORED_PLACEHOLDER_SESSION_TITLES
            .iter()
            .map(|title| (*title).to_string())
            .collect();
        for title in IGNORED_PLACEHOLDER_SESSION_TITLES {
            if *title == DEFAULT_TERMINAL_SESSION_TITLE.to_lowercase() {
                continue;
            }
            if let Some(base) = title.strip_suffix(" session") {
                titles.insert(format!("{base} agent session"));
            }
        }
        for name in DEFAULT_SESSION_AGENT_TITLE_NAMES {
            let normalized = collapse_js_whitespace(name).to_lowercase();
            if normalized.is_empty() {
                continue;
            }
            titles.insert(format!("{normalized} session"));
            titles.insert(format!("{normalized} agent session"));
            if let Some(base) = normalized.strip_suffix(" cli") {
                titles.insert(format!("{base} agent session"));
            }
        }
        titles
    })
}

/// `isDefaultSessionSearchTitle`.
pub(crate) fn is_default_session_search_title(title: &str) -> bool {
    let normalized = collapse_js_whitespace(title);
    if normalized.is_empty() {
        return false;
    }
    is_ignored_placeholder_session_title(&normalized)
}

fn is_ignored_placeholder_session_title(title: &str) -> bool {
    let normalized = collapse_js_whitespace(title);
    let lower = normalized.to_lowercase();
    is_session_number(&lower)
        || codex_session_id_from_title(&normalized)
        || is_ghost_placeholder(&normalized)
        || is_agent_status_word(&normalized)
        || placeholder_titles().contains(&lower)
        || is_path_like(&normalized)
}

/// `/^Session \d+$/iu`.
fn is_session_number(lower: &str) -> bool {
    lower
        .strip_prefix("session ")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

/// `/^👻(?:\s+Terminal Session)?$/u`.
fn is_ghost_placeholder(title: &str) -> bool {
    let Some(rest) = title.strip_prefix('👻') else {
        return false;
    };
    rest.is_empty()
        || (rest.starts_with(crate::sidebar_view::text::is_js_whitespace)
            && js_trim(rest) == "Terminal Session")
}

/// `AGENT_STATUS_WORD_TITLE_PATTERN`.
fn is_agent_status_word(title: &str) -> bool {
    let is_separator =
        |c: char| crate::sidebar_view::text::is_js_whitespace(c) || ".:[](){}!|/\\_-".contains(c);
    let core = title.trim_matches(is_separator);
    ["done", "error", "idle", "thinking", "working"]
        .iter()
        .any(|word| core.eq_ignore_ascii_case(word))
}

/// `isPathLikeTerminalTitle`.
fn is_path_like(title: &str) -> bool {
    title.starts_with('~')
        || title.starts_with('/')
        || ["…/", "…\\", ".../", "...\\"]
            .iter()
            .any(|prefix| title.starts_with(prefix))
}

/// `getCodexSessionIdFromTitle(title) !== undefined`.
fn codex_session_id_from_title(title: &str) -> bool {
    let Some(normalized) = normalize_terminal_title(title) else {
        return false;
    };
    let groups: Vec<&str> = normalized.split('-').collect();
    groups.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&groups)
            .all(|(len, group)| group.len() == *len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `LEADING_TERMINAL_TITLE_STATUS_MARKER_PATTERN`'s character class.
fn is_status_marker(c: char) -> bool {
    crate::sidebar_view::text::is_js_whitespace(c)
        || ('\u{2800}'..='\u{28FF}').contains(&c)
        || "·•⋅◦✳*∗✶✻✽✸✹✺✷✴◐◑◒◓✦◇🤖🔔".contains(c)
}

/// `normalizeTerminalTitle`.
fn normalize_terminal_title(title: &str) -> Option<String> {
    let trimmed = js_trim(title);
    if trimmed.is_empty() {
        return None;
    }
    let without_markers = trimmed.trim_start_matches(is_status_marker);
    let sanitized = js_trim(strip_oc_prefixes(without_markers));
    // Freebuff's `Freebuff: <prompt>` title (CDXC:SessionTitles in server/src/agents/terminal_title.rs).
    let sanitized = sanitized
        .strip_prefix("Freebuff: ")
        .map(js_trim)
        .unwrap_or(sanitized);
    // DeepSeek Harness's `✦ 🐋 <title>` (CDXC:SessionTitles in server/src/agents/terminal_title.rs).
    let sanitized = sanitized
        .strip_prefix('\u{1f40b}')
        .map(js_trim)
        .unwrap_or(sanitized);
    // Empryo's `<tab> · working — Empryo (beta)` title (CDXC:SessionTitles in server/src/agents/terminal_title.rs).
    let sanitized = strip_empryo_title_suffix(sanitized);
    // Codex's `[ ! ] Action Required | <title>` status title (CDXC:SessionTitles in server/src/agents/terminal_title.rs).
    let sanitized = strip_codex_action_required_prefix(sanitized);
    // Copilot's `<title> - GitHub Copilot` title (CDXC:SessionTitles in server/src/agents/terminal_title.rs).
    let sanitized = strip_copilot_title_suffix(sanitized);
    if let Some(cursor) = normalize_cursor_title(sanitized) {
        return cursor;
    }
    if is_antigravity_title(sanitized) {
        return Some("agy".to_string());
    }
    if let Some(pi) = normalize_pi_title(sanitized) {
        return Some(pi);
    }
    // Grok Build's `<status> - <title> - grok` title (CDXC:SessionTitles in server/src/agents/terminal_title.rs).
    if let Some(grok) = normalize_grok_title(sanitized) {
        return Some(grok);
    }
    (!sanitized.is_empty()).then(|| sanitized.to_string())
}

/// Same rule as gxserver's `normalize_grok_terminal_title`.
fn normalize_grok_title(title: &str) -> Option<String> {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let split_at = normalized.len().checked_sub(" - grok".len())?;
    if !normalized.is_char_boundary(split_at)
        || !normalized[split_at..].eq_ignore_ascii_case(" - grok")
    {
        return None;
    }
    let mut body = normalized[..split_at].trim();
    if let Some(after) = body.strip_prefix("\u{26a0} Action Required") {
        body = after.trim_start();
        body = body.strip_prefix('-').map_or(body, str::trim_start);
    }
    let unspun = body.trim_start_matches(|ch: char| ('\u{2800}'..='\u{28ff}').contains(&ch));
    let body = if unspun.len() != body.len() {
        unspun.trim_start()
    } else {
        body
    };
    let title = match body.strip_prefix('-') {
        Some(rest) => match rest.trim().split_once(" - ") {
            Some((status, title)) if !status.is_empty() => title.trim(),
            _ => "",
        },
        None => match body.split_once(" - ") {
            Some((status, title)) if is_grok_status_segment(status) => title.trim(),
            _ => body,
        },
    };
    Some(if title.is_empty() { "grok" } else { title }.to_string())
}

/// Same rule as gxserver's `is_grok_status_segment`.
fn is_grok_status_segment(segment: &str) -> bool {
    let lower = segment.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "cancelling"
            | "compacting"
            | "completed"
            | "done"
            | "error"
            | "executing"
            | "idle"
            | "responding"
            | "running"
            | "starting"
            | "stopped"
            | "thinking"
            | "verifying"
            | "waiting"
            | "working"
    ) || lower.starts_with("retrying")
}

/// Same rule as gxserver's `strip_copilot_title_suffix`.
fn strip_copilot_title_suffix(title: &str) -> &str {
    let trimmed = title.trim_end();
    if trimmed == "GitHub Copilot" {
        return "Copilot";
    }
    match trimmed.strip_suffix(" - GitHub Copilot") {
        Some(body) if !body.trim().is_empty() => body.trim_end(),
        _ => title,
    }
}

/// Same rule as gxserver's `strip_codex_action_required_prefix`.
fn strip_codex_action_required_prefix(title: &str) -> &str {
    let Some(rest) = title.trim_start().strip_prefix('[') else {
        return title;
    };
    let Some((marker, after)) = rest.split_once(']') else {
        return title;
    };
    if marker.chars().count() > 5 {
        return title;
    }
    let Some(after) = after.trim_start().strip_prefix("Action Required") else {
        return title;
    };
    let after = after.trim_start();
    after.strip_prefix('|').unwrap_or(after).trim()
}

/// Same rule as gxserver's `strip_empryo_title_suffix`.
fn strip_empryo_title_suffix(title: &str) -> &str {
    const SUFFIX: &str = " \u{2014} empryo";
    let trimmed = title.trim_end();
    // Most titles are not Empryo's; skip the lower-casing for them.
    if !trimmed
        .as_bytes()
        .windows("empryo".len())
        .any(|window| window.eq_ignore_ascii_case(b"empryo"))
    {
        return title;
    }
    // Rows from before the built-in agent read ` — empryo`; ASCII folding keeps byte offsets.
    let folded = trimmed.to_ascii_lowercase();
    let body = if is_empryo_channel_tail(folded.strip_prefix("empryo")) {
        ""
    } else {
        match folded.rfind(SUFFIX) {
            Some(index) if is_empryo_channel_tail(Some(&folded[index + SUFFIX.len()..])) => {
                &trimmed[..index]
            }
            _ => return title,
        }
    };
    let body = body.trim_end();
    let body = body
        .strip_suffix(" \u{00b7} working")
        .unwrap_or(body)
        .trim_end();
    if body.is_empty() {
        "Empryo"
    } else {
        body
    }
}

fn is_empryo_channel_tail(tail: Option<&str>) -> bool {
    match tail {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix(" (")
            .and_then(|inner| inner.strip_suffix(')'))
            .is_some_and(|inner| !inner.is_empty() && !inner.contains(['(', ')'])),
        None => false,
    }
}

/// `/^(?:OC\s*\|\s*)+/iu`.
fn strip_oc_prefixes(mut value: &str) -> &str {
    loop {
        let bytes = value.as_bytes();
        if bytes.len() < 2 || !bytes[..2].eq_ignore_ascii_case(b"oc") {
            return value;
        }
        let rest = value[2..].trim_start_matches(crate::sidebar_view::text::is_js_whitespace);
        let Some(rest) = rest.strip_prefix('|') else {
            return value;
        };
        value = rest.trim_start_matches(crate::sidebar_view::text::is_js_whitespace);
    }
}

fn is_antigravity_title(title: &str) -> bool {
    let normalized = collapse_js_whitespace(title);
    if normalized.eq_ignore_ascii_case("agy") {
        return true;
    }
    normalized.strip_prefix('🔔').is_some_and(|rest| {
        js_trim(rest).eq_ignore_ascii_case("agy") && !rest.trim_start().is_empty()
    })
}

fn is_cursor_placeholder(title: &str) -> bool {
    let normalized = collapse_js_whitespace(title);
    let lower = normalized.to_lowercase();
    if ["cursor", "cursor agent", "cursor cli", "cursor-agent"].contains(&lower.as_str()) {
        return true;
    }
    // `/^Cursor Agent\s*-\s*✅ Ready$/iu`.
    lower
        .strip_prefix("cursor agent")
        .and_then(|rest| js_trim(rest).strip_prefix('-'))
        .is_some_and(|rest| js_trim(rest) == "✅ ready")
}

/// `normalizeCursorTerminalTitle`: `Some(None)` is a cursor placeholder, `None` is "not cursor".
fn normalize_cursor_title(title: &str) -> Option<Option<String>> {
    let normalized = collapse_js_whitespace(title);
    if is_cursor_placeholder(&normalized) {
        return Some(None);
    }
    let strip = |suffix_start: usize| -> Option<Option<String>> {
        let head = &normalized[..suffix_start];
        let head = head.trim_end_matches(crate::sidebar_view::text::is_js_whitespace);
        let head = head.strip_suffix('-')?;
        let stripped = js_trim(head).to_string();
        Some(if is_cursor_placeholder(&stripped) || stripped.is_empty() {
            None
        } else {
            Some(stripped)
        })
    };
    if normalized.ends_with("✅ Ready") {
        let start = normalized.len() - "✅ Ready".len();
        // `CURSOR_CLI_READY_TITLE_STRIP_PATTERN` only removes a ` - ✅ Ready` suffix; without the
        // dash the whole title is kept.
        return Some(
            strip(start).unwrap_or_else(|| {
                Some(js_trim(&normalized).to_string()).filter(|t| !t.is_empty())
            }),
        );
    }
    if let Some(index) = normalized.rfind("⏳ Working ") {
        let dots = &normalized[index + "⏳ Working ".len()..];
        if !dots.is_empty() && dots.chars().all(|c| c == '.' || c == '·') {
            return Some(strip(index).unwrap_or_else(|| {
                Some(js_trim(&normalized).to_string()).filter(|t| !t.is_empty())
            }));
        }
    }
    None
}

/// `normalizePiTerminalTitle`.
fn normalize_pi_title(title: &str) -> Option<String> {
    let normalized = js_trim(title);
    let rest = normalized.strip_prefix('π')?;
    let after = rest.trim_start_matches(crate::sidebar_view::text::is_js_whitespace);
    if let Some(first) = after.chars().next() {
        if first == '>' || ('\u{2800}'..='\u{28FF}').contains(&first) {
            let body = js_trim(&after[first.len_utf8()..]);
            return Some(if body.is_empty() {
                "π".to_string()
            } else {
                body.to_string()
            });
        }
        if first == '-' {
            let body = &after[1..];
            let body = body.trim_start_matches(crate::sidebar_view::text::is_js_whitespace);
            if body.is_empty() {
                return None;
            }
            let parts: Vec<&str> = split_dash_parts(body)
                .into_iter()
                .map(js_trim)
                .filter(|part| !part.is_empty())
                .collect();
            if parts.len() < 2 {
                return Some("π".to_string());
            }
            let joined = parts[..parts.len() - 1].join(" - ");
            return Some(if joined.is_empty() {
                "π".to_string()
            } else {
                joined
            });
        }
    }
    None
}

/// `value.split(/\s+-\s+/u)`.
fn split_dash_parts(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    let mut start = 0;
    let mut index = 0;
    while index < chars.len() {
        let (byte, c) = chars[index];
        if crate::sidebar_view::text::is_js_whitespace(c) {
            let mut cursor = index;
            while cursor < chars.len()
                && crate::sidebar_view::text::is_js_whitespace(chars[cursor].1)
            {
                cursor += 1;
            }
            if cursor < chars.len() && chars[cursor].1 == '-' {
                let mut after = cursor + 1;
                let whitespace_after = after < chars.len()
                    && crate::sidebar_view::text::is_js_whitespace(chars[after].1);
                while after < chars.len()
                    && crate::sidebar_view::text::is_js_whitespace(chars[after].1)
                {
                    after += 1;
                }
                if whitespace_after {
                    parts.push(&value[start..byte]);
                    start = if after < chars.len() {
                        chars[after].0
                    } else {
                        value.len()
                    };
                    index = after;
                    continue;
                }
            }
        }
        index += 1;
    }
    parts.push(&value[start..]);
    parts
}
