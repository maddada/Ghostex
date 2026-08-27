use serde_json::Value;

use crate::ids::is_gxserver_session_id;

use super::*;

pub(crate) fn session_title_source(session: &Value, title: &str) -> String {
    read_runtime_text(session, "titleSource")
        .or_else(|| read_runtime_text(session, "restoreTitleSource"))
        .filter(|value| {
            matches!(
                value.as_str(),
                "browser-auto" | "generated" | "placeholder" | "terminal-auto" | "user"
            )
        })
        .unwrap_or_else(|| {
            if is_temporary_session_title(title) {
                "placeholder".to_string()
            } else {
                "user".to_string()
            }
        })
}

pub(crate) fn trusted_resume_title(title: &str, title_source: &str) -> Option<String> {
    if title_source == "placeholder" {
        return None;
    }
    let resume_title = visible_terminal_title(Some(title))?.trim().to_string();
    if resume_title.is_empty() || is_rejected_resume_title(&resume_title) {
        return None;
    }
    Some(resume_title)
}

pub(crate) fn session_card_primary_title(title: &str, agent_id: Option<&str>) -> Option<String> {
    let normalized = normalize_terminal_title(Some(title))
        .map(|title| normalize_spaces(title.trim()))
        .unwrap_or_else(|| normalize_spaces(title.trim()));
    if normalized.is_empty()
        || is_session_number_title(&normalized)
        || is_ignored_generic_agent_terminal_title(&normalized)
        || is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
    {
        return Some(agent_default_title(agent_id));
    }
    Some(normalized)
}

pub(crate) fn format_display_session_title(
    is_primary_title_terminal_title: bool,
    primary_title: Option<&str>,
    terminal_title: Option<&str>,
    title: &str,
    include_unsynced_title_label: bool,
) -> String {
    let normalized_primary_title = normalize_display_title(primary_title);
    let normalized_terminal_title = normalize_display_title(terminal_title);
    let normalized_title = normalize_display_title(Some(title));
    let base_title = normalized_primary_title
        .clone()
        .or(normalized_title)
        .unwrap_or_else(|| DEFAULT_TERMINAL_SESSION_TITLE.to_string());
    if is_primary_title_terminal_title
        || normalized_primary_title.is_none()
        || normalized_primary_title == normalized_terminal_title
    {
        return base_title;
    }
    if include_unsynced_title_label {
        format!("{TERMINAL_TITLE_MARKER} {base_title} {UNSYNCED_TITLE_LABEL}")
    } else {
        format!("{TERMINAL_TITLE_MARKER} {base_title}")
    }
}

pub(crate) fn normalize_display_title(title: Option<&str>) -> Option<String> {
    let normalized = normalize_spaces(title?.trim());
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub(crate) fn visible_terminal_title(title: Option<&str>) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    if is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
        || is_ignored_placeholder_session_title(&normalized)
        || is_ignored_generic_agent_terminal_title(&normalized)
        || is_agent_status_word_title(&normalized)
        || is_windows_default_powershell_title(&normalized)
    {
        return None;
    }
    Some(normalized)
}

pub(crate) fn normalize_terminal_title(title: Option<&str>) -> Option<String> {
    let normalized = title?.trim();
    if normalized.is_empty() {
        return None;
    }
    let without_markers = normalized
        .trim_start_matches(is_leading_terminal_title_status_marker)
        .trim();
    let sanitized = strip_oc_prefixes(without_markers).trim().to_string();
    if let Some(cursor_title) = normalize_cursor_terminal_title(&sanitized) {
        return cursor_title;
    }
    if let Some(antigravity_title) = normalize_antigravity_terminal_title(&sanitized) {
        return antigravity_title;
    }
    if let Some(pi_title) = normalize_pi_terminal_title(&sanitized) {
        return Some(pi_title);
    }
    if let Some(grok_title) = normalize_grok_terminal_title(&sanitized) {
        return Some(grok_title);
    }
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

pub(crate) fn is_leading_terminal_title_status_marker(ch: char) -> bool {
    /*
    CDXC:GxserverSessionTitles 2026-06-29-01:21:
    Factory Droid terminal titles can prefix visible session names with the U+26EC status marker.
    Presentation must strip the marker for existing stored rows too, because sidebar copy/details reads displayTitle before the raw durable title.
    */
    ch.is_whitespace()
        || ('\u{2800}'..='\u{28ff}').contains(&ch)
        || matches!(
            ch,
            '\u{00b7}'
                | '\u{2022}'
                | '\u{22c5}'
                | '\u{25e6}'
                | '\u{2733}'
                | '*'
                | '\u{2217}'
                | '\u{2736}'
                | '\u{273b}'
                | '\u{273d}'
                | '\u{2738}'
                | '\u{2739}'
                | '\u{273a}'
                | '\u{2737}'
                | '\u{2734}'
                | '\u{25d0}'
                | '\u{25d1}'
                | '\u{25d2}'
                | '\u{25d3}'
                | '\u{26ec}'
                | '\u{2726}'
                | '\u{25c7}'
                | '\u{1f916}'
                | '\u{1f514}'
        )
}

pub(crate) fn strip_oc_prefixes(title: &str) -> String {
    let mut rest = title;
    loop {
        let lower = rest.to_lowercase();
        if !lower.starts_with("oc") {
            break;
        }
        let after_oc = &rest[2..];
        let after_spaces = after_oc.trim_start();
        let Some(after_pipe) = after_spaces.strip_prefix('|') else {
            break;
        };
        rest = after_pipe.trim_start();
    }
    rest.to_string()
}

pub(crate) fn normalize_cursor_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title.trim());
    if is_cursor_cli_placeholder_terminal_title(&normalized) {
        return Some(None);
    }
    if normalized.ends_with("\u{2705} Ready") {
        let stripped = strip_cursor_status_suffix(&normalized, "\u{2705} Ready");
        return Some(cursor_status_title(stripped));
    }
    let working_marker = "\u{23f3} Working ";
    if let Some(index) = normalized.rfind(working_marker) {
        let trailing = &normalized[index + working_marker.len()..];
        if !trailing.is_empty() && trailing.chars().all(|ch| ch == '.' || ch == '\u{00b7}') {
            let stripped = strip_cursor_working_suffix(&normalized, index);
            return Some(cursor_status_title(stripped));
        }
    }
    None
}

pub(crate) fn cursor_status_title(stripped: String) -> Option<String> {
    if is_cursor_cli_placeholder_terminal_title(&stripped) {
        return None;
    }
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

pub(crate) fn strip_cursor_status_suffix(title: &str, suffix: &str) -> String {
    let Some(prefix) = title.strip_suffix(suffix) else {
        return title.trim().to_string();
    };
    prefix
        .trim_end()
        .strip_suffix('-')
        .map(str::trim)
        .unwrap_or(title)
        .trim()
        .to_string()
}

pub(crate) fn strip_cursor_working_suffix(title: &str, status_index: usize) -> String {
    let prefix = &title[..status_index];
    prefix
        .trim_end()
        .strip_suffix('-')
        .map(str::trim)
        .unwrap_or(title)
        .trim()
        .to_string()
}

pub(crate) fn is_cursor_cli_placeholder_terminal_title(title: &str) -> bool {
    let normalized = normalize_spaces(title.trim());
    let lower = normalized.to_lowercase();
    lower == "cursor"
        || lower == "cursor agent"
        || lower == "cursor cli"
        || lower == "cursor-agent"
        || lower == "cursor agent - \u{2705} ready"
}

pub(crate) fn normalize_antigravity_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title.trim());
    let lower = normalized.to_lowercase();
    if lower == "agy" {
        return Some(Some("agy".to_string()));
    }
    if let Some(rest) = normalized.strip_prefix('\u{1f514}') {
        if rest.trim().eq_ignore_ascii_case("agy") {
            return Some(Some("agy".to_string()));
        }
    }
    None
}

pub(crate) fn normalize_pi_terminal_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    let rest = trimmed.strip_prefix('\u{03c0}')?.trim_start();
    if let Some(status_marker) = rest.chars().next() {
        if status_marker == '>' || ('\u{2800}'..='\u{28ff}').contains(&status_marker) {
            let title = rest[status_marker.len_utf8()..].trim();
            return Some(if title.is_empty() {
                "\u{03c0}".to_string()
            } else {
                title.to_string()
            });
        }
    }
    let rest = rest.strip_prefix('-')?.trim();
    if rest.is_empty() {
        return None;
    }
    let parts = rest
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        Some("\u{03c0}".to_string())
    } else {
        Some(parts[..parts.len() - 1].join(" - "))
    }
}

pub(crate) const GROK_TERMINAL_TITLE_SUFFIX: &str = " - grok";

/*
Grok Build publishes `{spinner} - {status} - {session title} - grok` while it
works and `{session title} - grok` when it is idle. Tabs and sidebar rows only
want the session title, so drop the trailing agent segment and the leading
status segment instead of showing `- Thinking - Fix bun run start command - grok`.
*/
pub(crate) fn normalize_grok_terminal_title(title: &str) -> Option<String> {
    let normalized = normalize_spaces(title.trim());
    let split_at = normalized
        .len()
        .checked_sub(GROK_TERMINAL_TITLE_SUFFIX.len())?;
    if !normalized.is_char_boundary(split_at) {
        return None;
    }
    let (body, suffix) = normalized.split_at(split_at);
    if !suffix.eq_ignore_ascii_case(GROK_TERMINAL_TITLE_SUFFIX) {
        return None;
    }
    let body = strip_grok_status_prefix(body.trim());
    Some(if body.is_empty() {
        "grok".to_string()
    } else {
        body
    })
}

pub(crate) fn strip_grok_status_prefix(body: &str) -> String {
    if let Some(rest) = body.strip_prefix('-') {
        let rest = rest.trim();
        return match rest.split_once(" - ") {
            Some((status, title)) if !status.is_empty() => title.trim().to_string(),
            _ => String::new(),
        };
    }
    match body.split_once(" - ") {
        Some((status, title)) if is_grok_status_segment(status) => title.trim().to_string(),
        _ => body.to_string(),
    }
}

pub(crate) fn is_grok_status_segment(segment: &str) -> bool {
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

pub(crate) fn is_ignored_placeholder_session_title(title: &str) -> bool {
    let normalized = normalize_spaces(title.trim());
    let lower = normalized.to_lowercase();
    is_session_number_title(&normalized)
        || codex_session_id_from_title(&normalized).is_some()
        || is_ghost_placeholder_session_title(&normalized)
        || is_agent_status_word_title(&normalized)
        || is_ignored_placeholder_session_title_text(&lower)
        || is_path_like_terminal_title(&normalized)
}

pub(crate) fn is_ignored_placeholder_session_title_text(lower: &str) -> bool {
    matches!(
        lower,
        "terminal session"
            | "amp cli session"
            | "amp session"
            | "antigravity cli session"
            | "antigravity session"
            | "campfire session"
            | "claude session"
            | "claude code session"
            | "codebuddy session"
            | "code buddy session"
            | "codex session"
            | "codex cli session"
            | "command code session"
            | "commandcode session"
            | "copilot session"
            | "cursor agent session"
            | "cursor cli session"
            | "cursor session"
            | "devin session"
            | "droid session"
            | "factory droid session"
            | "gemini session"
            | "grok session"
            | "grok build session"
            | "hermes session"
            | "hermes agent session"
            | "kimi session"
            | "kimi code session"
            | "kiro session"
            | "kiro cli session"
            | "omp session"
            | "openclaude session"
            | "open claude session"
            | "opencode session"
            | "open code session"
            | "openai codex session"
            | "pi session"
            | "qoder session"
            | "qodercli session"
            | "rovo session"
            | "rovo dev session"
            | "rovodev session"
    )
}

pub(crate) fn is_ignored_generic_agent_terminal_title(title: &str) -> bool {
    let lower = normalize_spaces(title.trim()).to_lowercase();
    matches!(
        lower.as_str(),
        "amp"
            | "amp cli"
            | "agy"
            | "antigravity"
            | "antigravity cli"
            | "claude"
            | "claude code"
            | "codex"
            | "codex cli"
            | "cursor"
            | "cursor agent"
            | "cursor cli"
            | "cursor-agent"
            | "droid"
            | "factory droid"
            | "grok"
            | "grok build"
            | "kiro"
            | "kiro cli"
            | "kiro-cli"
            | "omp"
            | "openai codex"
            | "pi"
            | "\u{03c0}"
            | "ghostex"
    )
}

pub(crate) fn is_rejected_resume_title(title: &str) -> bool {
    let normalized = title.trim();
    let lower = normalized.to_lowercase();
    normalized == "\u{00f0}^\u{00df}^\u{00d1}\u{00bb}"
        || is_temporary_session_title(normalized)
        || is_ghost_placeholder_session_title(normalized)
        || is_gxserver_session_id(normalized)
        || normalized
            .chars()
            .any(|ch| (ch as u32) <= 0x1f || (ch as u32) == 0x7f)
        || (normalized.starts_with('\u{00f0}') && normalized.ends_with('\u{00bb}'))
        || is_agent_command_noise_title(&lower)
}

pub(crate) fn is_agent_command_noise_title(title: &str) -> bool {
    let Some(executable_name) = command_executable_name(title) else {
        return false;
    };
    if !is_agent_command_executable_name(&executable_name) {
        return false;
    }
    if title == executable_name {
        return true;
    }
    let rest = title[executable_name.len()..].trim();
    if rest.is_empty() || rest.starts_with('-') {
        return true;
    }
    let first_arg = rest.split_whitespace().next().unwrap_or_default();
    is_agent_command_subcommand_name(first_arg)
}

pub(crate) fn command_executable_name(command: &str) -> Option<String> {
    let first = command.split_whitespace().next()?.trim();
    let first = first.trim_matches(|ch| ch == '\'' || ch == '"');
    if first.is_empty() {
        None
    } else {
        Some(first.to_lowercase())
    }
}

pub(crate) fn is_agent_command_executable_name(value: &str) -> bool {
    matches!(
        value,
        "acli"
            | "agy"
            | "amp"
            | "campfire"
            | "claude"
            | "codebuddy"
            | "codex"
            | "commandcode"
            | "copilot"
            | "cursor-agent"
            | "devin"
            | "droid"
            | "gemini"
            | "grok"
            | "hermes"
            | "kimi"
            | "kiro-cli"
            | "omp"
            | "openclaude"
            | "opencode"
            | "pi"
            | "qodercli"
    )
}

pub(crate) fn is_agent_command_subcommand_name(value: &str) -> bool {
    matches!(
        value,
        "auth"
            | "completion"
            | "debug"
            | "exec"
            | "help"
            | "login"
            | "logout"
            | "mcp"
            | "resume"
            | "run"
            | "sandbox"
            | "session"
            | "sessions"
    )
}

pub(crate) fn codex_session_id_from_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(Some(title))?;
    if is_uuid_like(&normalized) {
        Some(normalized.to_lowercase())
    } else {
        None
    }
}

pub(crate) fn is_uuid_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

pub(crate) fn is_ghost_placeholder_session_title(title: &str) -> bool {
    let normalized = normalize_spaces(title.trim());
    normalized == "\u{1f47b}" || normalized == "\u{1f47b} Terminal Session"
}

pub(crate) fn is_temporary_session_title(title: &str) -> bool {
    normalize_spaces(title.trim()).to_lowercase() == "search by text"
}

pub(crate) fn is_session_number_title(title: &str) -> bool {
    let lower = normalize_spaces(title.trim()).to_lowercase();
    let Some(rest) = lower.strip_prefix("session ") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

pub(crate) fn is_path_like_terminal_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.starts_with('~')
        || trimmed.starts_with('/')
        || trimmed.starts_with("\u{2026}/")
        || trimmed.starts_with("\u{2026}\\")
        || trimmed.starts_with(".../")
        || trimmed.starts_with("...\\")
}

pub(crate) fn is_shell_location_terminal_title(title: &str) -> bool {
    let Some((user_host, location)) = title.split_once(':') else {
        return false;
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return false;
    };
    if user.trim().is_empty()
        || host.trim().is_empty()
        || user.chars().any(char::is_whitespace)
        || host.chars().any(char::is_whitespace)
    {
        return false;
    }
    let location = location.trim_start();
    is_path_like_terminal_title(location) || is_windows_absolute_terminal_path(location)
}

pub(crate) fn is_windows_absolute_terminal_path(title: &str) -> bool {
    let bytes = title.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || title.starts_with("\\\\")
}

pub(crate) fn is_agent_status_word_title(title: &str) -> bool {
    let core = title
        .trim_matches(is_agent_status_boundary_char)
        .to_lowercase();
    matches!(
        core.as_str(),
        "done" | "error" | "idle" | "thinking" | "working"
    )
}

pub(crate) fn is_agent_status_boundary_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '.' | ':' | '[' | ']' | '(' | ')' | '{' | '}' | '!' | '|' | '/' | '\\' | '_' | '-'
        )
}

pub(crate) fn is_windows_default_powershell_title(title: &str) -> bool {
    let lower = title.to_lowercase();
    let bytes = lower.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let rest = &lower[1..];
    let prefix = ":\\windows\\system32\\windowspowershell\\v1.0\\powershell.exe";
    let Some(suffix) = rest.strip_prefix(prefix) else {
        return false;
    };
    suffix.is_empty() || (suffix.starts_with(char::is_whitespace) && suffix.trim() == ".")
}

pub(crate) fn agent_default_title(agent_id: Option<&str>) -> String {
    let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return DEFAULT_TERMINAL_SESSION_TITLE.to_string();
    };
    let normalized = agent_id.to_lowercase().replace(['-', '_'], " ");
    let title = normalized
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let word = if first.is_ascii_alphabetic() {
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            } else {
                format!("{first}{}", chars.as_str())
            };
            if word == "Cli" {
                "CLI".to_string()
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{title} Session")
}
