use super::*;

pub(super) fn classify_terminal_title_status(
    title: Option<&str>,
    known_agent_name: Option<&str>,
) -> Option<TitleStatusSignal> {
    let title = title?;
    let normalized_agent_name = normalize_status_agent_name(known_agent_name);
    let normalized_title = normalize_spaces(title);
    if is_opencode_title_prefix(&normalized_title) {
        return None;
    }
    if let Some(agent_name) = normalized_agent_name.as_deref() {
        let state = match agent_name {
            "antigravity" => get_antigravity_title_state(title, true),
            "claude" => get_claude_code_title_state(title, true),
            "codex" => get_codex_title_state(title, true),
            "copilot" => get_copilot_title_state(title, true),
            "cursor" => get_cursor_title_state(title, true),
            "gemini" => get_gemini_title_state(title, true),
            "grok" => get_grok_title_state(title),
            "pi" => get_pi_title_state(title, true),
            /*
            CDXC:AgentScreenDetection 2026-10-06 WHY:
            Empryo titles its terminal "<tab> · working — Empryo (beta)". The middle dot is one of Claude Code's spinner frames, so the scan for unrecognised agents read every Empryo title as Claude's and the next title tick idled the turn Empryo's hooks had just started (observed live 2026-10-06, session G8p0d). Empryo's status comes from its hooks alone, so its titles carry no status signal.
            */
            /*
            CDXC:AgentScreenDetection 2026-10-16 WHY:
            ZCode 3.15 titles its terminal "ZC | ⠋ <activity>" for as long as a submission is active, including while it waits at its permission picker, so the spinner cannot tell working from waiting on the user. Unrecognised, the title fell through to the other agents' spinner scans, which claimed it for another agent; the attention a PermissionRequest hook had set turned into that agent's working, and the identity check then dropped the record as the wrong agent's, leaving the session idle while ZCode waited for an answer (observed live 2026-10-16, throwaway G0n25). ZCode's hooks report working, attention and finished turns, so its titles carry no status signal.
            */
            /*
            CDXC:AgentScreenDetection 2026-10-17 WHY:
            OMP 18.11 paints a spinner title while it runs a tool. Unrecognised, it fell through to the other agents' spinner scans, which claimed it for Codex; the identity check then dropped the record as the wrong agent's, so the sidebar went idle mid-turn and never reached attention (observed live 2026-10-17, throwaway G2ow5). Ghostex's OMP extension reports prompts, tools, questions and finished turns through hooks, so its titles carry no status signal.
            */
            /*
            CDXC:ClaudeCloud 2026-10-23 WHY:
            A Claude cloud session's terminal runs Ghostex's own cloud client (or, once Claude allows it, `claude --cloud <link>` attached to the cloud), and its status comes from the session's branch and pull request and from its claude.ai page (server/src/claude_cloud/). An attached Claude would paint Claude's spinner titles, which would override that status, so its titles carry no status signal.
            */
            /*
            CDXC:AgentScreenDetection 2026-10-28 WHY:
            DeepSeek Harness's terminal app titles its terminal `⠂ 🐋 <title>` while it works and `✦ 🐋 <title>` when idle. Unrecognised, the braille and the four-pointed star fell through to Claude's, Codex's and Gemini's scans, and the next title tick idled the turn its session log had just started (observed live 2026-10-28, throwaway G5usd). Its session log (server/src/dsh_activity.rs) and its OSC 7501 reports own its status, so its titles carry no status signal.
            */
            "opencode" | "empryo" | "zcode" | "omp" | "claude-cloud" | "dsh" => None,
            _ => None,
        };
        return state.map(|state| signal(agent_name, state));
    }
    if let Some(state) = get_grok_title_state(title) {
        return Some(signal("grok", state));
    }
    if let Some(state) = get_cursor_title_state(title, false) {
        return Some(signal("cursor", state));
    }
    if let Some(state) = get_antigravity_title_state(title, false) {
        return Some(signal("antigravity", state));
    }
    if let Some(state) = get_claude_code_title_state(title, false) {
        return Some(signal("claude", state));
    }
    if let Some(state) = get_pi_title_state(title, false) {
        return Some(signal("pi", state));
    }
    if let Some(state) = get_codex_title_state(title, false) {
        return Some(signal("codex", state));
    }
    if let Some(state) = get_gemini_title_state(title, false) {
        return Some(signal("gemini", state));
    }
    if let Some(state) = get_copilot_title_state(title, false) {
        return Some(signal("copilot", state));
    }
    None
}

/// Agents whose terminal titles say nothing about their activity; their hooks own it.
pub(super) fn titles_carry_no_status(agent_name: Option<&str>) -> bool {
    matches!(
        agent_name,
        Some("opencode" | "empryo" | "zcode" | "omp" | "claude-cloud" | "dsh")
    )
}

pub(super) fn signal(agent_name: &str, state: &str) -> TitleStatusSignal {
    TitleStatusSignal {
        agent_name: agent_name.to_string(),
        state: state.to_string(),
    }
}

pub(super) fn normalize_status_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    let mapped = match normalized.as_str() {
        "claude code" => "claude",
        "codex cli" => "codex",
        "github copilot" => "copilot",
        "agy" | "antigravity cli" | "antigravity" => "antigravity",
        "cursor cli" | "cursor-agent" | "cursor agent" => "cursor",
        "grok build" => "grok",
        "open code" => "opencode",
        "\u{03c0}" => "pi",
        other => other,
    };
    matches!(
        mapped,
        "antigravity"
            | "claude"
            | "codex"
            | "cursor"
            | "gemini"
            | "copilot"
            | "grok"
            | "opencode"
            | "empryo"
            | "pi"
            | "omp"
            | "zcode"
            | "claude-cloud"
            | "dsh"
    )
    .then(|| mapped.to_string())
}

pub(super) fn requires_observed_title_transitions(agent_name: Option<&str>) -> bool {
    matches!(
        agent_name,
        Some("claude" | "codex" | "cursor" | "grok" | "pi")
    )
}

pub(super) fn get_title_activity_window_ms(agent_name: Option<&str>) -> i64 {
    if requires_observed_title_transitions(agent_name) {
        SLOW_SPINNER_ACTIVITY_WINDOW_MS
    } else {
        TITLE_ACTIVITY_WINDOW_MS
    }
}

pub(super) fn create_title_activity_signature(
    title: Option<&str>,
    signal: Option<&TitleStatusSignal>,
) -> Option<String> {
    let normalized_title = title
        .map(normalize_spaces)
        .filter(|value| !value.is_empty())?;
    if !matches!(
        signal.map(|signal| signal.state.as_str()),
        Some("working" | "attention")
    ) {
        return Some(normalized_title);
    }
    let mut chars = normalized_title.chars().collect::<Vec<_>>();
    match signal.map(|signal| signal.agent_name.as_str()) {
        Some("codex" | "pi") => {
            for ch in &mut chars {
                if CODEX_WORKING_MARKERS.contains(ch) {
                    *ch = ' ';
                }
            }
        }
        Some("claude") => {
            for ch in &mut chars {
                if CLAUDE_CODE_WORKING_MARKERS.contains(ch) || CLAUDE_CODE_IDLE_MARKERS.contains(ch)
                {
                    *ch = ' ';
                }
            }
        }
        _ => {}
    }
    let mut signature = chars.into_iter().collect::<String>();
    if matches!(
        signal.map(|signal| signal.agent_name.as_str()),
        Some("codex" | "pi")
    ) && is_codex_action_required_title(&signature)
    {
        signature = "Action Required".to_string();
    }
    if signal.map(|signal| signal.agent_name.as_str()) == Some("cursor") {
        signature = replace_cursor_working_suffix(&signature);
    }
    if signal.map(|signal| signal.agent_name.as_str()) == Some("grok") {
        signature =
            crate::presentation::normalize_grok_terminal_title(&signature).unwrap_or(signature);
    }
    Some(collapse_signature_noise(&signature))
}

fn get_grok_title_state(title: &str) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let split_at = normalized
        .len()
        .checked_sub(crate::presentation::GROK_TERMINAL_TITLE_SUFFIX.len())?;
    if !normalized.is_char_boundary(split_at) {
        return None;
    }
    let (body, suffix) = normalized.split_at(split_at);
    if !suffix.eq_ignore_ascii_case(crate::presentation::GROK_TERMINAL_TITLE_SUFFIX) {
        return None;
    }

    // CDXC:SessionTitles 2026-10-17 WHY: while a permission waits Grok blinks `⚠ Action Required - ` in front of its spinner, so every other frame read as idle and the session flapped between working and idle for as long as the card stayed open.
    if body.trim_start().starts_with("\u{26a0} Action Required") {
        return Some("working");
    }
    let status_stripped =
        body.trim_start_matches(crate::presentation::is_leading_terminal_title_status_marker);
    if status_stripped.len() != body.len()
        || status_stripped
            .strip_prefix('-')
            .map(str::trim_start)
            .is_some_and(|rest| rest.contains(" - "))
    {
        Some("working")
    } else {
        Some("idle")
    }
}

fn get_cursor_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if lower == "cursor agent - \u{2705} ready" || normalized.ends_with("\u{2705} Ready") {
        return Some("idle");
    }
    if normalized.ends_with_working_suffix() {
        return Some("working");
    }
    if lower == "cursor agent" {
        return Some("idle");
    }
    let has_cursor_keyword = lower.contains("cursor cli")
        || lower.contains("cursor-agent")
        || lower.contains("cursor agent")
        || lower == "cursor";
    (allow_agent_hint_match && has_cursor_keyword).then_some("idle")
}

pub(super) fn is_cursor_ready_title(title: &str) -> bool {
    normalize_spaces(title).ends_with("\u{2705} Ready")
}

/// The chat name in front of Cursor's " - ⏳ Working ···" / " - ✅ Ready"
/// status suffix, or the whole title when it carries no such suffix.
pub(super) fn cursor_title_chat_name(title: &str) -> String {
    let normalized = normalize_spaces(title);
    let end = normalized
        .find("\u{23f3} Working")
        .or_else(|| normalized.find("\u{2705} Ready"))
        .unwrap_or(normalized.len());
    normalized[..end]
        .trim_end()
        .strip_suffix('-')
        .unwrap_or(&normalized[..end])
        .trim()
        .to_string()
}

trait CursorTitleExt {
    fn ends_with_working_suffix(&self) -> bool;
}

impl CursorTitleExt for str {
    /*
    CDXC:SessionStatus 2026-09-03:
    Cursor's spinner cycles "⏳ Working ···", ".··", "..·", "...", so the
    suffix is the marker followed by any mix of dots and middle dots. The old
    check matched only the "..." frame, which left most working titles
    unclassified and the Ready-title stop below without a working baseline.
    */
    fn ends_with_working_suffix(&self) -> bool {
        const MARKER: &str = "\u{23f3} Working";
        let Some(index) = self.rfind(MARKER) else {
            return false;
        };
        self[index + MARKER.len()..]
            .chars()
            .all(|ch| matches!(ch, ' ' | '.' | '\u{b7}'))
    }
}

fn get_codex_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    let has_codex_keyword = lower.contains("codex");
    let has_codex_working_marker = get_codex_working_marker(&normalized).is_some();
    if allow_agent_hint_match && is_codex_action_required_title(&normalized) {
        return Some("attention");
    }
    if !allow_agent_hint_match && !has_codex_keyword && !has_codex_working_marker {
        return None;
    }
    if has_codex_working_marker {
        Some("working")
    } else {
        Some("idle")
    }
}

/// CDXC:SessionStatus 2026-10-16 WHY:
/// Claude's agents view titles the terminal "2 awaiting input · claude agents"; its middle dot is one of Claude's spinner frames, so the view read as a working turn and an error retry waited behind it without saying why. The view says nothing about the conversation's own turn, so it gives no status.
fn is_claude_agents_view_title(normalized: &str) -> bool {
    let lower = normalized.trim().to_ascii_lowercase();
    lower == "claude agents" || lower.ends_with("· claude agents")
}

fn get_claude_code_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    if is_claude_agents_view_title(&normalize_spaces(title)) {
        return None;
    }
    if get_cursor_title_state(title, false).is_some()
        || get_codex_title_state(title, true) == Some("attention")
    {
        return None;
    }
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    let has_claude_keyword = lower.contains("claude code") || lower.contains("claude");
    let has_inference_marker = contains_any_marker(&normalized, CLAUDE_CODE_IDLE_MARKERS)
        || contains_any_marker(&normalized, CLAUDE_CODE_WORKING_MARKERS);
    if !allow_agent_hint_match && !has_claude_keyword && !has_inference_marker {
        return None;
    }
    if contains_any_marker(&normalized, CLAUDE_CODE_IDLE_MARKERS) {
        return Some("idle");
    }
    if contains_any_marker(&normalized, CLAUDE_CODE_WORKING_MARKERS) {
        return Some("working");
    }
    has_claude_keyword.then_some("idle")
}

fn get_pi_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let has_prefix = pi_title_prefix(&normalized);
    if !allow_agent_hint_match && !has_prefix {
        return None;
    }
    if get_codex_working_marker(&normalized).is_some() {
        return Some("working");
    }
    (has_prefix || allow_agent_hint_match).then_some("idle")
}

fn get_gemini_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if !allow_agent_hint_match
        && !lower.contains("gemini")
        && !normalized.contains('\u{2726}')
        && !normalized.contains('\u{25c7}')
    {
        return None;
    }
    if normalized.contains('\u{2726}') {
        return Some("working");
    }
    normalized.contains('\u{25c7}').then_some("idle")
}

fn get_antigravity_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if lower == "\u{1f514} agy" {
        return Some("attention");
    }
    if lower == "agy" {
        return Some("idle");
    }
    (allow_agent_hint_match
        && (lower == "antigravity" || lower == "antigravity cli" || lower == "agy"))
        .then_some("idle")
}

fn get_copilot_title_state(title: &str, allow_agent_hint_match: bool) -> Option<&'static str> {
    if get_antigravity_title_state(title, false).is_some() {
        return None;
    }
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if !allow_agent_hint_match
        && !lower.contains("copilot")
        && !lower.contains("github copilot")
        && !normalized.contains('\u{1f916}')
        && !normalized.contains('\u{1f514}')
    {
        return None;
    }
    if normalized.contains('\u{1f916}') {
        return Some("working");
    }
    normalized.contains('\u{1f514}').then_some("idle")
}

fn is_opencode_title_prefix(title: &str) -> bool {
    let stripped = trim_title_prefix_markers(title);
    stripped.to_ascii_lowercase().starts_with("oc |")
        || stripped.to_ascii_lowercase().starts_with("oc|")
}

fn pi_title_prefix(title: &str) -> bool {
    trim_title_prefix_markers(title).starts_with("\u{03c0} -")
        || trim_title_prefix_markers(title).starts_with("\u{03c0}-")
}

fn trim_title_prefix_markers(value: &str) -> &str {
    value.trim_start_matches(|ch: char| {
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
                    | '\u{25d0}'
                    | '\u{25d1}'
                    | '\u{25d2}'
                    | '\u{25d3}'
                    | '\u{2726}'
                    | '\u{25c7}'
                    | '\u{1f916}'
                    | '\u{1f514}'
            )
    })
}

fn is_codex_action_required_title(title: &str) -> bool {
    let trimmed = title.trim_start();
    let Some(rest) = trimmed.strip_prefix('[') else {
        return false;
    };
    let Some((marker, after)) = rest.split_once(']') else {
        return false;
    };
    marker
        .chars()
        .any(|ch| matches!(ch, '!' | '.' | '\u{00b7}' | '\u{2802}'))
        && after.trim_start().starts_with("Action Required")
}

fn get_codex_working_marker(title: &str) -> Option<char> {
    title.chars().find(|ch| CODEX_WORKING_MARKERS.contains(ch))
}

fn contains_any_marker(title: &str, markers: &[char]) -> bool {
    title.chars().any(|ch| markers.contains(&ch))
}

fn replace_cursor_working_suffix(value: &str) -> String {
    let normalized = normalize_spaces(value);
    if let Some(index) = normalized.find("\u{23f3} Working") {
        format!("{}Working", normalized[..index].trim_end())
    } else {
        normalized
    }
}

fn collapse_signature_noise(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            let mut digits = String::from(ch);
            while let Some(next) = chars.peek().copied() {
                if next.is_ascii_digit() {
                    digits.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            let mut lookahead = chars.clone();
            let mut saw_space = false;
            while let Some(next) = lookahead.peek().copied() {
                if next.is_whitespace() {
                    saw_space = true;
                    lookahead.next();
                } else {
                    break;
                }
            }
            if lookahead.peek().copied() == Some('s') && saw_space {
                while let Some(next) = chars.peek().copied() {
                    if next.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek().copied() == Some('s') {
                    chars.next();
                    result.push_str("<elapsed>");
                    continue;
                }
            }
            result.push_str(&digits);
            continue;
        }
        if ch.is_whitespace()
            || ('\u{2800}'..='\u{28ff}').contains(&ch)
            || matches!(
                ch,
                '\u{00b7}'
                    | '\u{2022}'
                    | '\u{22c5}'
                    | '\u{25e6}'
                    | '\u{2733}'
                    | '*'
                    | '\u{25d0}'
                    | '\u{25d1}'
                    | '\u{25d2}'
                    | '\u{25d3}'
                    | '\u{2726}'
                    | '\u{25c7}'
                    | '\u{1f916}'
                    | '\u{1f514}'
            )
        {
            result.push(' ');
        } else {
            result.push(ch);
        }
    }
    normalize_spaces(&result)
}

pub(super) fn normalize_activity_event(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| {
            matches!(
                *value,
                "launch"
                    | "resume"
                    | "wake"
                    | "escape"
                    | "agentDetected"
                    | "title"
                    | "bell"
                    | "terminalError"
                    | "terminalExited"
                    | "acknowledge"
            )
        })
        .map(str::to_string)
}

pub(super) fn create_attention_event_id(now_ms_value: i64) -> String {
    format!("attn_{}", to_base36(now_ms_value))
}

fn to_base36(value: i64) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let negative = value < 0;
    let mut value = value.unsigned_abs();
    let mut digits = Vec::new();
    while value > 0 {
        let digit = (value % 36) as u8;
        digits.push(match digit {
            0..=9 => (b'0' + digit) as char,
            _ => (b'a' + digit - 10) as char,
        });
        value /= 36;
    }
    digits.reverse();
    let mut output = digits.into_iter().collect::<String>();
    if negative {
        output.insert(0, '-');
    }
    output
}
