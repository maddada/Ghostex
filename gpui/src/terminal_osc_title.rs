//! Rust port of the terminal-title trust rules from
//! `shared/session-grid-contract-session.ts` (`getVisibleTerminalTitle`).
//! Live Ghostty OSC titles may only replace GPUI tab labels when the shared
//! sidebar contract would also accept them as visible terminal titles, so
//! spinner glyphs, cwd paths, agent boot placeholders, and ghost reconnect
//! titles cannot poison native tab chrome. Keep the rules here in sync with
//! the shared module.

use std::collections::HashSet;
use std::sync::OnceLock;

const LEADING_STATUS_MARKER_CHARS: &str = "·•⋅◦✳*∗✶✻✽✸✹✺✷✴✦◇🤖🔔";

const IGNORED_GENERIC_TERMINAL_TITLES: &[&str] = &[
    "amp",
    "amp cli",
    "agy",
    "antigravity",
    "antigravity cli",
    "claude",
    "claude code",
    "codex",
    "codex cli",
    "cursor",
    "cursor agent",
    "cursor cli",
    "cursor-agent",
    "droid",
    "factory droid",
    "grok",
    "grok build",
    "openai codex",
    "kiro",
    "kiro cli",
    "kiro-cli",
    "omp",
    "pi",
    "π",
    "ghostex",
];

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
    "copilot session",
    "cursor agent session",
    "cursor cli session",
    "cursor session",
    "droid session",
    "factory droid session",
    "gemini session",
    "grok session",
    "grok build session",
    "hermes session",
    "hermes agent session",
    "kiro session",
    "kiro cli session",
    "omp session",
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

const DEFAULT_SESSION_AGENT_TITLE_NAMES: &[&str] = &[
    "antigravity cli",
    "amp cli",
    "claude",
    "codebuddy",
    "codex",
    "copilot",
    "cursor cli",
    "factory droid",
    "gemini",
    "grok build",
    "hermes agent",
    "kiro cli",
    "omp",
    "opencode",
    "pi",
    "qoder",
    "rovo dev",
];

const AGENT_STATUS_WORDS: &[&str] = &["done", "error", "idle", "thinking", "working"];

fn placeholder_session_titles() -> &'static HashSet<String> {
    static TITLES: OnceLock<HashSet<String>> = OnceLock::new();
    TITLES.get_or_init(|| {
        let mut titles: HashSet<String> = IGNORED_PLACEHOLDER_SESSION_TITLES
            .iter()
            .map(|title| (*title).to_string())
            .collect();
        for title in IGNORED_PLACEHOLDER_SESSION_TITLES {
            if *title == "terminal session" {
                continue;
            }
            if let Some(base) = title.strip_suffix(" session") {
                titles.insert(format!("{base} agent session"));
            }
        }
        for agent_title_name in DEFAULT_SESSION_AGENT_TITLE_NAMES {
            titles.insert(format!("{agent_title_name} session"));
            titles.insert(format!("{agent_title_name} agent session"));
            if let Some(base) = agent_title_name.strip_suffix(" cli") {
                titles.insert(format!("{base} agent session"));
            }
        }
        titles
    })
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_leading_status_marker(c: char) -> bool {
    c.is_whitespace()
        || ('\u{2800}'..='\u{28FF}').contains(&c)
        || LEADING_STATUS_MARKER_CHARS.contains(c)
}

fn strip_oc_prefixes(value: &str) -> &str {
    let mut rest = value;
    loop {
        let after_oc = rest.trim_start();
        let Some(after_oc) = after_oc
            .strip_prefix("OC")
            .or_else(|| after_oc.strip_prefix("oc"))
            .or_else(|| after_oc.strip_prefix("Oc"))
            .or_else(|| after_oc.strip_prefix("oC"))
        else {
            return rest;
        };
        let Some(after_pipe) = after_oc.trim_start().strip_prefix('|') else {
            return rest;
        };
        rest = after_pipe.trim_start();
    }
}

fn is_cursor_cli_placeholder_title(title: &str) -> bool {
    let collapsed = collapse_whitespace(title).to_lowercase();
    if matches!(
        collapsed.as_str(),
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent"
    ) {
        return true;
    }
    if let Some(rest) = collapsed.strip_prefix("cursor agent") {
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('-').unwrap_or(rest).trim_start();
        return rest == "✅ ready";
    }
    false
}

fn strip_cursor_status_suffix(title: &str) -> Option<String> {
    let strip_dash_and_trim = |value: &str| {
        let value = value.trim_end();
        value
            .strip_suffix('-')
            .unwrap_or(value)
            .trim_end()
            .to_string()
    };
    if let Some(prefix) = title.trim_end().strip_suffix("✅ Ready") {
        return Some(strip_dash_and_trim(prefix));
    }
    let trailing_dots = title
        .chars()
        .rev()
        .take_while(|c| *c == '.' || *c == '·')
        .count();
    if trailing_dots > 0 {
        let without_dots = &title[..title.len()
            - title
                .chars()
                .rev()
                .take(trailing_dots)
                .map(char::len_utf8)
                .sum::<usize>()];
        if let Some(prefix) = without_dots.strip_suffix("⏳ Working ") {
            return Some(strip_dash_and_trim(prefix));
        }
    }
    None
}

fn normalize_antigravity_title(title: &str) -> Option<String> {
    let collapsed = collapse_whitespace(title).to_lowercase();
    let stripped = collapsed.strip_prefix("🔔").map(str::trim_start);
    if collapsed == "agy" || stripped == Some("agy") {
        return Some("agy".to_string());
    }
    None
}

fn normalize_pi_title(title: &str) -> Option<String> {
    let rest = title.trim().strip_prefix('π')?;
    let rest = rest.trim_start();
    if let Some(status_marker) = rest.chars().next() {
        if status_marker == '>' || ('\u{2800}'..='\u{28ff}').contains(&status_marker) {
            let title = rest[status_marker.len_utf8()..].trim();
            return Some(if title.is_empty() {
                "π".to_string()
            } else {
                title.to_string()
            });
        }
    }
    let rest = rest.strip_prefix('-')?.trim_start();
    if rest.is_empty() {
        return None;
    }
    let collapsed = collapse_whitespace(rest);
    let parts = collapsed
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Some("π".to_string());
    }
    let named = parts[..parts.len() - 1].join(" - ");
    Some(if named.is_empty() {
        "π".to_string()
    } else {
        named
    })
}

fn normalize_terminal_title(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = trimmed.trim_start_matches(is_leading_status_marker);
    let sanitized = strip_oc_prefixes(sanitized).trim();

    if is_cursor_cli_placeholder_title(sanitized) {
        return None;
    }
    if let Some(stripped) = strip_cursor_status_suffix(sanitized) {
        if stripped.is_empty() || is_cursor_cli_placeholder_title(&stripped) {
            return None;
        }
        return Some(stripped);
    }
    if let Some(antigravity) = normalize_antigravity_title(sanitized) {
        return Some(antigravity);
    }
    if let Some(pi) = normalize_pi_title(sanitized) {
        return Some(pi);
    }
    (!sanitized.is_empty()).then(|| sanitized.to_string())
}

fn is_path_like_title(title: &str) -> bool {
    title.starts_with('~')
        || title.starts_with('/')
        || ["…/", "…\\", ".../", "...\\"]
            .iter()
            .any(|prefix| title.starts_with(prefix))
}

fn is_session_number_title(collapsed_lower: &str) -> bool {
    collapsed_lower
        .strip_prefix("session ")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn is_codex_session_id_title(title: &str) -> bool {
    let groups = title.split('-').collect::<Vec<_>>();
    groups.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&groups)
            .all(|(len, group)| group.len() == *len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_ghost_placeholder_title(collapsed: &str) -> bool {
    collapsed == "👻" || collapsed == "👻 Terminal Session"
}

fn is_agent_status_word_title(title: &str) -> bool {
    let is_separator = |c: char| c.is_whitespace() || ".:[](){}!|/\\_-".contains(c);
    let core = title.trim_matches(is_separator);
    !core.is_empty()
        && AGENT_STATUS_WORDS
            .iter()
            .any(|word| core.eq_ignore_ascii_case(word))
}

fn is_windows_default_powershell_title(collapsed_lower: &str) -> bool {
    let mut chars = collapsed_lower.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    let rest = chars.as_str();
    let Some(rest) =
        rest.strip_prefix(":\\windows\\system32\\windowspowershell\\v1.0\\powershell.exe")
    else {
        return false;
    };
    rest.is_empty() || rest.trim_start() == "." && rest.starts_with(char::is_whitespace)
}

fn is_ignored_placeholder_session_title(title: &str) -> bool {
    let collapsed = collapse_whitespace(title);
    let collapsed_lower = collapsed.to_lowercase();
    is_session_number_title(&collapsed_lower)
        || is_codex_session_id_title(&collapsed_lower)
        || is_ghost_placeholder_title(&collapsed)
        || is_agent_status_word_title(&collapsed)
        || placeholder_session_titles().contains(&collapsed_lower)
        || is_path_like_title(&collapsed)
}

/// Port of the shared contract's `getVisibleTerminalTitle`: returns the
/// normalized title only when it is trustworthy enough to display.
pub(crate) fn visible_terminal_osc_title(raw: &str) -> Option<String> {
    let normalized = normalize_terminal_title(raw)?;
    if is_path_like_title(&normalized) {
        return None;
    }
    if is_ignored_placeholder_session_title(&normalized) {
        return None;
    }
    let normalized_lower = normalized.trim().to_lowercase();
    if IGNORED_GENERIC_TERMINAL_TITLES.contains(&normalized_lower.as_str()) {
        return None;
    }
    if is_agent_status_word_title(&normalized) {
        return None;
    }
    if is_windows_default_powershell_title(&normalized_lower) {
        return None;
    }
    Some(normalized)
}
