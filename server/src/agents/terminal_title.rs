use crate::ids::is_gxserver_session_id;
use crate::presentation::normalize_pi_terminal_title;
use serde_json::Value;

use super::*;

pub(crate) fn get_visible_terminal_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    if is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
        || is_ignored_placeholder_session_title(&normalized)
        || is_generic_agent_title(&normalized)
        || is_status_word_title(&normalized)
        || is_windows_default_powershell_title(&normalized)
    {
        return None;
    }
    Some(normalized)
}

pub(crate) fn normalize_terminal_title(title: &str) -> Option<String> {
    let value = title.trim();
    if value.is_empty() {
        return None;
    }
    let value = strip_oc_prefixes(value.trim_start_matches(is_leading_title_marker).trim());
    if let Some(cursor) = normalize_cursor_terminal_title(&value) {
        return cursor;
    }
    if let Some(antigravity) = normalize_antigravity_terminal_title(&value) {
        return antigravity;
    }
    if let Some(pi) = normalize_pi_terminal_title(&value) {
        return Some(pi);
    }
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

pub(crate) fn get_codex_session_id_from_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    is_uuid(&normalized).then(|| normalized.to_ascii_lowercase())
}

pub(crate) fn terminal_title_indicates_agent_identity(title: &str) -> bool {
    get_codex_session_id_from_title(title).is_some()
        || get_terminal_title_detected_agent_name(title).is_some()
}

pub(crate) fn is_uuid(value: &str) -> bool {
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

pub(crate) fn trusted_resume_title(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let title = read_text_value(session, "title")?;
    if normalize_title_source(
        read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .as_deref(),
        &title,
    ) == "placeholder"
    {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_resume_title(&visible)).then_some(visible)
}

pub(crate) fn is_rejected_resume_title(title: &str) -> bool {
    let normalized = title.trim();
    let lower = normalized.to_ascii_lowercase();
    normalized == "\u{00f0}^\u{00df}^\u{00d1}\u{00bb}"
        || is_temporary_title(normalized)
        || is_ghost_placeholder_session_title(normalized)
        || is_gxserver_session_id(title.trim())
        || normalized
            .chars()
            .any(|ch| (ch as u32) <= 0x1f || (ch as u32) == 0x7f)
        || (normalized.starts_with('\u{00f0}') && normalized.ends_with('\u{00bb}'))
        || is_agent_command_noise_title(&lower)
}

pub(crate) fn is_temporary_title(title: &str) -> bool {
    normalize_spaces(title).eq_ignore_ascii_case("search by text")
}

pub(crate) fn is_ignored_placeholder_session_title(title: &str) -> bool {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    is_session_number_title(&normalized)
        || get_codex_session_id_from_title(&normalized).is_some()
        || is_ghost_placeholder_session_title(&normalized)
        || is_status_word_title(&normalized)
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
            | "claude code session"
            | "claude session"
            | "code buddy session"
            | "codebuddy session"
            | "codex cli session"
            | "codex session"
            | "copilot session"
            | "cursor agent session"
            | "cursor cli session"
            | "cursor session"
            | "droid session"
            | "factory droid session"
            | "gemini session"
            | "grok build session"
            | "grok session"
            | "hermes agent session"
            | "hermes session"
            | "kiro cli session"
            | "kiro session"
            | "omp session"
            | "open code session"
            | "openai codex session"
            | "opencode session"
            | "pi session"
            | "qoder session"
            | "qodercli session"
            | "rovo dev session"
            | "rovo session"
            | "rovodev session"
    )
}

pub(crate) fn is_generic_agent_title(title: &str) -> bool {
    let lower = normalize_spaces(title).to_ascii_lowercase();
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

pub(crate) fn is_status_word_title(title: &str) -> bool {
    let core = title
        .trim_matches(is_agent_status_boundary_char)
        .to_ascii_lowercase();
    matches!(
        core.as_str(),
        "done" | "error" | "idle" | "thinking" | "working"
    )
}

pub(crate) fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn is_leading_title_marker(ch: char) -> bool {
    /*
    CDXC:GxserverSessionTitles 2026-06-29-01:21:
    Factory Droid terminal titles can prefix visible session names with the U+26EC status marker.
    Strip it at the same boundary as Claude, Codex, Cursor, Gemini, and Copilot title chrome so copied details and sidebar rows show the semantic title instead of provider decoration.
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
        let lower = rest.to_ascii_lowercase();
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
    let normalized = normalize_spaces(title);
    if is_cursor_placeholder_title(&normalized) {
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
    if stripped.is_empty() || is_cursor_placeholder_title(&stripped) {
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

pub(crate) fn is_cursor_placeholder_title(title: &str) -> bool {
    let lower = normalize_spaces(title).to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" | "cursor agent - \u{2705} ready"
    )
}

pub(crate) fn normalize_antigravity_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
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

pub(crate) fn is_ghost_placeholder_session_title(title: &str) -> bool {
    matches!(
        normalize_spaces(title).as_str(),
        "\u{1f47b}" | "\u{1f47b} Terminal Session"
    )
}

pub(crate) fn is_session_number_title(title: &str) -> bool {
    let lower = normalize_spaces(title).to_ascii_lowercase();
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
    /*
    Interactive WSL shells commonly publish `user@host: /path` as their OSC
    title. That describes the terminal location, not the Ghostex session, and
    must never replace Terminal Session, an agent placeholder, or a generated
    first-prompt title.
    */
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

pub(crate) fn is_agent_status_boundary_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '.' | ':' | '[' | ']' | '(' | ')' | '{' | '}' | '!' | '|' | '/' | '\\' | '_' | '-'
        )
}

pub(crate) fn is_windows_default_powershell_title(title: &str) -> bool {
    let lower = title.to_ascii_lowercase();
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
    (!first.is_empty()).then(|| first.to_ascii_lowercase())
}

pub(crate) fn is_agent_command_executable_name(value: &str) -> bool {
    matches!(
        value,
        "acli"
            | "agy"
            | "amp"
            | "claude"
            | "codebuddy"
            | "codex"
            | "copilot"
            | "cursor-agent"
            | "droid"
            | "gemini"
            | "grok"
            | "hermes"
            | "kiro-cli"
            | "omp"
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

pub(crate) fn get_terminal_title_detected_agent_name(title: &str) -> Option<String> {
    let normalized = normalize_spaces(title);
    [
        "antigravity",
        "claude",
        "codex",
        "copilot",
        "cursor",
        "gemini",
        "pi",
    ]
    .into_iter()
    .find(|agent| is_explicit_agent_program_terminal_title(&normalized, agent))
    .map(str::to_string)
}

pub(crate) fn is_explicit_agent_program_terminal_title(title: &str, agent_name: &str) -> bool {
    match agent_name {
        "antigravity" => {
            let lower = normalize_spaces(title).to_ascii_lowercase();
            lower == "agy" || lower == "\u{1f514} agy"
        }
        "claude" => strip_one_leading_title_marker(title).eq_ignore_ascii_case("Claude Code"),
        "codex" => matches!(
            strip_braille_dot_prefix(title)
                .to_ascii_lowercase()
                .as_str(),
            "codex" | "codex cli"
        ),
        "copilot" => matches!(
            strip_specific_prefix_markers(title, &['\u{1f916}', '\u{1f514}'])
                .to_ascii_lowercase()
                .as_str(),
            "copilot" | "copilot cli" | "github copilot" | "github copilot cli"
        ),
        "cursor" => matches!(
            normalize_spaces(title).to_ascii_lowercase().as_str(),
            "cursor agent" | "cursor agent - \u{2705} ready"
        ),
        "gemini" => matches!(
            strip_specific_prefix_markers(title, &['\u{2726}', '\u{25c7}'])
                .to_ascii_lowercase()
                .as_str(),
            "gemini" | "gemini cli"
        ),
        "pi" => {
            let stripped = title.trim_start_matches(is_leading_title_marker).trim();
            stripped.starts_with("\u{03c0} -") || stripped.starts_with("\u{03c0}-")
        }
        _ => false,
    }
}

pub(crate) fn strip_one_leading_title_marker(title: &str) -> String {
    let trimmed = title.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if is_leading_title_marker(first) {
        chars.as_str().trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn strip_braille_dot_prefix(title: &str) -> String {
    title
        .trim_start_matches(|ch| {
            ('\u{2800}'..='\u{28ff}').contains(&ch)
                || matches!(ch, '\u{00b7}' | '\u{2022}' | '\u{22c5}' | '\u{25e6}')
                || ch.is_whitespace()
        })
        .trim()
        .to_string()
}

pub(crate) fn strip_specific_prefix_markers(title: &str, markers: &[char]) -> String {
    title
        .trim_start_matches(|ch: char| ch.is_whitespace() || markers.contains(&ch))
        .trim()
        .to_string()
}
