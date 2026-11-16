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
        || is_windows_console_default_title(&normalized)
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
    let value = strip_freebuff_title_prefix(&value);
    let value = strip_dsh_title_prefix(value);
    let value = strip_empryo_title_suffix(value);
    let value = strip_codex_action_required_prefix(value);
    let value = strip_copilot_title_suffix(value).to_string();
    if let Some(cursor) = normalize_cursor_terminal_title(&value) {
        return cursor;
    }
    if let Some(antigravity) = normalize_antigravity_terminal_title(&value) {
        return antigravity;
    }
    if let Some(pi) = normalize_pi_terminal_title(&value) {
        return Some(pi);
    }
    // CDXC:SessionTitles 2026-10-17 WHY: the sidebar already dropped Grok Build's status segment and ` - grok` suffix, but stored titles, and the notifications and kill messages read from them, kept both ("- Waiting for response… - <title> - grok"); the stored title uses the presentation rule too.
    if let Some(grok) = crate::presentation::normalize_grok_terminal_title(&value) {
        return Some(grok);
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
    let title_source = normalize_title_source(
        read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .as_deref(),
        &title,
    );
    if title_source == "placeholder" || is_terminal_auto_working_directory_title(session) {
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
            | "command code session"
            | "commandcode session"
            | "copilot session"
            | "cursor agent session"
            | "cursor cli session"
            | "cursor session"
            | "devin session"
            | "droid session"
            | "factory droid session"
            | "freebuff session"
            | "deepseek harness session"
            | "dsh session"
            | "grok build session"
            | "grok session"
            | "hermes agent session"
            | "hermes session"
            | "kimi code session"
            | "kimi session"
            | "kiro cli session"
            | "kiro session"
            | "omp session"
            | "open claude session"
            | "open code session"
            | "openai codex session"
            | "openclaude session"
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
            | "copilot"
            | "cursor"
            | "cursor agent"
            | "cursor cli"
            | "cursor-agent"
            | "droid"
            | "empryo"
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
    CDXC:SessionTitles 2026-06-29-01:21:
    Factory Droid terminal titles can prefix visible session names with the U+26EC status marker.
    Strip it at the same boundary as Claude, Codex, Cursor, and Copilot title chrome so copied details and sidebar rows show the semantic title instead of provider decoration.
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

/// CDXC:SessionTitles 2026-10-04 DECISION:
/// User: hide "Freebuff:" from the start of Freebuff's titles in the sidebar and everywhere else in the app ("Freebuff: what model is this?" shows as "what model is this?").
/// Freebuff titles its terminal `Freebuff: <first prompt>`; only that exact spelling is stripped, so other titles that mention Freebuff stay as they are.
/// SEE-ALSO: server/src/presentation/title_normalization.rs and packages/gx-core/src/quick_access/session_titles.rs normalize titles with the same rule.
pub(crate) fn strip_freebuff_title_prefix(title: &str) -> &str {
    title
        .strip_prefix("Freebuff: ")
        .map(str::trim_start)
        .unwrap_or(title)
}

/// CDXC:SessionTitles 2026-10-28 WHY:
/// DeepSeek Harness's terminal app titles its terminal `✦ 🐋 <title>` (a spinner in place of `✦` while it works). The leading glyph is already dropped as a status marker; the whale is its logo, not part of the name, so it is dropped too, the way Freebuff's `Freebuff: ` is.
/// SEE-ALSO: server/src/presentation/title_normalization.rs and packages/gx-core/src/quick_access/session_titles.rs normalize titles with the same rule.
pub(crate) fn strip_dsh_title_prefix(title: &str) -> &str {
    title
        .strip_prefix('\u{1f40b}')
        .map(str::trim_start)
        .unwrap_or(title)
}

/// CDXC:SessionTitles 2026-10-17 WHY:
/// While Codex waits for an approval it titles its terminal `[ ! ] Action Required | <its usual title>`, blinking to `[ . ] …` every second, and its usual title is the thread's id until Codex names the thread. A session adopted that whole status string as its name (`[ . ] Action Required | 01a148f2-…`, observed with codex-cli 0.163.0). The prefix is a status, never a name, so it is stripped and the remainder goes through the usual checks: an id or a bare prefix gives no title.
/// SEE-ALSO: server/src/presentation/title_normalization.rs and packages/gx-core/src/quick_access/session_titles.rs normalize titles with the same rule; `is_codex_action_required_title` in server/src/session_status/title_classify.rs reads the same prefix as attention.
pub(crate) fn strip_codex_action_required_prefix(title: &str) -> &str {
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

/// CDXC:SessionTitles 2026-10-18 WHY:
/// Codex's default terminal title is `<activity> <thread name> | <project name>`, the project name being the folder of the git root (else of the working folder) cut to 24 characters. Ghostex stored that whole string as the session's name, so a Codex session whose conversation id was never recorded resumed with `codex resume "<thread> | <project>"`, which names no thread, and Codex exited (GitHub issue #256). The project segment is Codex chrome, like Copilot's suffix: it is dropped, and a title that is only the project name (before Codex names the thread) gives no title. Returns `None` for that bare project name.
pub(crate) fn strip_codex_project_title_suffix(title: &str, cwd: Option<&str>) -> Option<String> {
    let title = title.trim();
    let Some(cwd) = cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) else {
        return Some(title.to_string());
    };
    let cwd = std::path::Path::new(cwd);
    let git_root = cwd
        .ancestors()
        .find(|dir| dir.join(".git").symlink_metadata().is_ok());
    let project_names = [git_root, Some(cwd)]
        .into_iter()
        .flatten()
        .filter_map(|dir| dir.file_name().and_then(|name| name.to_str()))
        .map(codex_terminal_title_project_name)
        .collect::<Vec<_>>();
    if project_names.iter().any(|name| name == title) {
        return None;
    }
    let stripped = project_names
        .iter()
        .find_map(|name| title.strip_suffix(name.as_str())?.strip_suffix(" | "))
        .unwrap_or(title)
        .trim();
    Some(stripped.to_string())
}

/// Codex's `truncate_terminal_title_part(project, 24)` (codex-rs/tui/src/chatwidget/status_surfaces.rs), by characters.
fn codex_terminal_title_project_name(name: &str) -> String {
    const MAX_CHARS: usize = 24;
    if name.chars().count() <= MAX_CHARS {
        return name.to_string();
    }
    let mut truncated = name.chars().take(MAX_CHARS - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

/// CDXC:SessionTitles 2026-10-17 WHY:
/// GitHub Copilot CLI titles its terminal `<title> - GitHub Copilot`, and a bare `GitHub Copilot` before its first prompt, so session rows kept the product name as part of their name ("Reply with OK - GitHub Copilot"). The suffix is provider decoration like Empryo's and is stripped; a bare title reads as the generic "Copilot" placeholder.
/// SEE-ALSO: server/src/presentation/title_normalization.rs and packages/gx-core/src/quick_access/session_titles.rs normalize titles with the same rule.
pub(crate) fn strip_copilot_title_suffix(title: &str) -> &str {
    let trimmed = title.trim_end();
    if trimmed == "GitHub Copilot" {
        return "Copilot";
    }
    match trimmed.strip_suffix(" - GitHub Copilot") {
        Some(body) if !body.trim().is_empty() => body.trim_end(),
        _ => title,
    }
}

/// CDXC:SessionTitles 2026-10-06 WHY:
/// Empryo titles its terminal `<tab label> — Empryo (beta)`, `✦ <tab label> · working — Empryo (beta)` while a turn runs, and a bare `Empryo (beta)` before its first tab mounts. The agent name and the working word are stripped like every other agent's, so the row shows the tab label alone and a bare title reads as the generic "Empryo" placeholder. The parenthesised channel is matched loosely so a later "(rc)" or no suffix at all still strips.
/// SEE-ALSO: server/src/presentation/title_normalization.rs and packages/gx-core/src/quick_access/session_titles.rs normalize titles with the same rule.
pub(crate) fn strip_empryo_title_suffix(title: &str) -> &str {
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
        || is_windows_absolute_terminal_path(trimmed)
}

pub(crate) fn is_session_working_directory_title(session: &Value, title: &str) -> bool {
    /*
    Linux shells and terminal integrations often publish only the final cwd
    component as OSC title text. Unlike `/home/me/project` or
    `me@host: ~/project`, that bare `project` label is not path-shaped, so the
    generic terminal-title filter cannot identify it. Compare it with the
    daemon-owned session cwd instead; callers still use title provenance to
    preserve an explicit user/generated title that happens to match the folder.
    Splitting on both separators keeps this check correct for native Windows,
    WSL, Linux, macOS, and remote sessions without host-specific branches.
    */
    let Some(normalized_title) = normalize_terminal_title(title) else {
        return false;
    };
    let Some(cwd) = read_text_value(session, "cwd") else {
        return false;
    };
    let trimmed_cwd = cwd.trim().trim_end_matches(['/', '\\']);
    let Some(directory_name) = trimmed_cwd
        .rsplit(['/', '\\'])
        .find(|part| !part.trim().is_empty())
    else {
        return false;
    };
    normalize_spaces(&normalized_title) == normalize_spaces(directory_name)
}

pub(crate) fn is_terminal_auto_working_directory_title(session: &Value) -> bool {
    let runtime_settings = object_field(session, "runtimeSettings");
    let title_source = normalize_title_source(
        read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .as_deref(),
        session
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    title_source == "terminal-auto"
        && read_text_value(session, "title")
            .as_deref()
            .is_some_and(|title| is_session_working_directory_title(session, title))
}

pub(crate) fn is_terminal_prompt_or_directory_title(session: &Value, title: Option<&str>) -> bool {
    let Some(title) = title else {
        return false;
    };
    if title.trim().is_empty() {
        return true;
    }
    let Some(normalized) = normalize_terminal_title(title) else {
        return false;
    };
    if is_generic_agent_title(&normalized)
        || is_status_word_title(&normalized)
        || get_codex_session_id_from_title(&normalized).is_some()
    {
        return false;
    }
    let lower = normalized.to_ascii_lowercase();
    is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
        || is_session_working_directory_title(session, title)
        || is_windows_console_default_title(&normalized)
        || is_ignored_placeholder_session_title_text(&lower)
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

/// CDXC:SessionTitles 2026-09-22 WHY:
/// Elevated ConPTY shells prefix their executable title with Administrator. That shell context must be filtered just like the unelevated path or waking a session overwrites its chosen name.
///
/// CDXC:SessionTitles 2026-10-04 DECISION:
/// User: new Freebuff sessions (and maybe the others) were titled "Windows PowerShell"; "this shouldn't be the case". The console's own default titles (`Windows PowerShell`, `PowerShell`) are shell context like the executable path, so they never name a session.
///
/// CDXC:SessionTitles 2026-10-14 DECISION:
/// User: "for WSL please fix the name of the sessions showing up like this" (sessions named `C:\WINDOWS\system32\wsl.exe`). A Windows console's default title is the path of the program that opened it, so with the WSL backend every terminal starts titled with the wsl.exe path. Any absolute `.exe` path is that console default, not a name, which also covers cmd.exe and a CLI launched directly; the 2026-10-04 rule only knew powershell.exe and pwsh.exe.
pub(crate) fn is_windows_console_default_title(title: &str) -> bool {
    let normalized = title.trim().to_ascii_lowercase().replace('/', "\\");
    let path = normalized
        .strip_prefix("administrator:")
        .unwrap_or(&normalized)
        .trim_start();
    if matches!(path, "windows powershell" | "powershell") {
        return true;
    }
    let path = path.strip_suffix(" .").unwrap_or(path).trim_end();
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1..3] == *b":\\"
        && path.ends_with(".exe")
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
            | "commandcode"
            | "copilot"
            | "cursor-agent"
            | "mastracode"
            | "devin"
            | "droid"
            | "empryo"
            | "freebuff"
            | "dsh-tui"
            | "grok"
            | "hermes"
            | "kimi"
            | "kiro-cli"
            | "omp"
            | "openclaude"
            | "opencode"
            | "pi"
            | "qodercli"
            | "zcode"
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
    ["antigravity", "claude", "codex", "copilot", "cursor", "pi"]
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

#[cfg(test)]
mod dsh_title_tests {
    use super::*;

    #[test]
    fn deepseek_harness_titles_drop_their_status_glyph_and_whale() {
        assert_eq!(
            normalize_terminal_title("✦ 🐋 Run GHOSTEX_DSH_OK echo command").as_deref(),
            Some("Run GHOSTEX_DSH_OK echo command")
        );
        assert_eq!(strip_dsh_title_prefix("🐋 Fix tests"), "Fix tests");
        assert_eq!(strip_dsh_title_prefix("Whales 🐋"), "Whales 🐋");
    }
}
