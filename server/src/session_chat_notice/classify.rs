use super::*;

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

fn signature_matches(screen: &NoticeScreen, signature: &NoticeSignature) -> bool {
    if signature.scope == NoticeScope::Exit && !screen.ends_with_shell_prompt() {
        return false;
    }
    let window = screen.window(signature.scope);
    if signature.scope == NoticeScope::Exit {
        // Exit signatures live in an even tighter window than banners.
        let exit_window = flatten_tail(&screen.folded, NOTICE_EXIT_SCAN_LINES);
        if !matches_parts(&exit_window, signature.parts) {
            return false;
        }
    } else if !matches_parts(window, signature.parts) {
        return false;
    }
    signature.corroborators.is_empty()
        || signature
            .corroborators
            .iter()
            .any(|corroborator| window.contains(corroborator))
}

fn notice_from_rule(
    screen: &NoticeScreen,
    rule: &NoticeRule,
    signature: &NoticeSignature,
) -> SessionChatTerminalNotice {
    let evidence = rule
        .quote_evidence
        .then(|| signature_needle(signature).and_then(|needle| screen.evidence(needle)))
        .flatten();
    SessionChatTerminalNotice::new(
        rule.kind,
        rule.severity,
        SessionChatTerminalNoticeSource::Screen,
        rule.title,
    )
    .with_input_blocking(rule.blocks_input)
    .with_detail(
        match evidence.as_deref().and_then(limit_reset).or_else(|| {
            (rule.detail == ANTIGRAVITY_USAGE_LIMIT_DETAIL)
                .then(|| limit_reset(screen.window(NoticeScope::Banner)))
                .flatten()
        }) {
            Some(reset)
                if rule.detail == CLAUDE_USAGE_LIMIT_DETAIL
                    || rule.detail == ANTIGRAVITY_USAGE_LIMIT_DETAIL =>
            {
                rule.detail
                    .replacen("usage limit.", &format!("usage limit ({reset})."), 1)
            }
            _ => rule.detail.to_string(),
        },
    )
    .with_details(evidence)
    .with_screen_tail(screen.screen_tail())
    .with_actions(
        rule.actions
            .iter()
            .map(|action| SessionChatTerminalNoticeAction {
                id: action.id.to_string(),
                label: action.label.to_string(),
                kind: action.kind,
                send: action.send.map(str::to_string),
            })
            .collect(),
    )
}

/// "resets 4pm" from Claude's "You've hit your limit · resets 4pm (Europe/Berlin)",
/// "resets in 3h16m25s" / "resets in 1h 23m" / "resets in 30 seconds" from Antigravity's banners,
/// or "retry in 30s" from Antigravity's rate limit banner, without the time zone.
fn limit_reset(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    for needle in ["resets in ", "retry in "] {
        if let Some(start) = lower.find(needle) {
            let rest = &line[start + needle.len()..];
            if let Some(phrase) = extract_duration_phrase(rest) {
                return Some(format!("{needle}{phrase}"));
            }
        }
    }
    if let Some(start) = lower.find("resets ") {
        let reset = &line[start..];
        let reset = match reset.find(" (") {
            Some(at) => &reset[..at],
            None => reset,
        };
        let reset = reset.split("Error ID:").next().unwrap_or(reset);
        let reset = reset.trim().trim_end_matches(['.', ',']);
        if reset.len() > "resets ".len() {
            let mut normalized = reset.to_string();
            if normalized.starts_with('R') {
                normalized.replace_range(0..1, "r");
            }
            return Some(normalized);
        }
    }
    None
}

fn is_duration_unit_word(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "s" | "sec"
            | "secs"
            | "second"
            | "seconds"
            | "m"
            | "min"
            | "mins"
            | "minute"
            | "minutes"
            | "h"
            | "hr"
            | "hrs"
            | "hour"
            | "hours"
            | "d"
            | "day"
            | "days"
    )
}

fn is_go_duration_token(token: &str) -> bool {
    crate::accounts::hub::cooldown::go_duration(token).is_some_and(|d| d > chrono::Duration::zero())
}

fn spaced_unit_seconds(num: i64, unit: &str) -> Option<i64> {
    if num <= 0 {
        return None;
    }
    match unit.to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => Some(num),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(num.saturating_mul(60)),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(num.saturating_mul(3600)),
        "d" | "day" | "days" => Some(num.saturating_mul(86400)),
        _ => None,
    }
}

/// Extracts one or more consecutive duration tokens from the start of `rest` (`"12m52s."`,
/// `"1h 23m."`, `"30 seconds."`, `"2 minutes 30 seconds."`), stopping at sentence punctuation.
fn extract_duration_phrase(rest: &str) -> Option<String> {
    let raw_words: Vec<&str> = rest.split_whitespace().take(6).collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < raw_words.len() {
        let raw = raw_words[i];
        let ends_sentence = raw.ends_with(['.', ',', ')', ';', '!', '?']);
        let clean = raw.trim_end_matches(['.', ',', ')', ';', '!', '?']);
        if clean.is_empty() {
            break;
        }
        if is_go_duration_token(clean) {
            parts.push(clean.to_string());
            i += 1;
            if ends_sentence {
                break;
            }
            continue;
        }
        if clean.chars().all(|c| c.is_ascii_digit()) && i + 1 < raw_words.len() {
            let next_raw = raw_words[i + 1];
            let next_ends = next_raw.ends_with(['.', ',', ')', ';', '!', '?']);
            let next_clean = next_raw.trim_end_matches(['.', ',', ')', ';', '!', '?']);
            if is_duration_unit_word(next_clean) {
                parts.push(format!("{clean} {next_clean}"));
                i += 2;
                if next_ends {
                    break;
                }
                continue;
            }
        }
        if parts.is_empty() {
            // Keep single fallback token for unparsed forms like "soon" or "4pm".
            parts.push(clean.to_string());
        }
        break;
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Parses a relative reset or retry duration from a usage-limit notice (`Resets in 3h16m25s`,
/// `Resets in 1h 23m`, `Resets in 30 seconds`, `retry in 30s`, etc.) using the shared Go duration
/// parser and spaced-unit accumulation.
pub(crate) fn parse_notice_reset_duration(
    notice: &SessionChatTerminalNotice,
) -> Option<chrono::Duration> {
    let tail = notice.screen_tail.as_deref().unwrap_or_default();
    let texts = [
        notice.details.clone().unwrap_or_default(),
        notice.detail.clone().unwrap_or_default(),
        tail.lines().map(str::trim).collect::<Vec<_>>().join(" "),
    ];
    texts
        .iter()
        .find_map(|text| parse_reset_duration_text(text))
}

pub(crate) fn parse_reset_duration_text(text: &str) -> Option<chrono::Duration> {
    let lower = text.to_ascii_lowercase();
    for needle in ["resets in ", "retry in "] {
        let mut offset = 0;
        while let Some(found) = lower[offset..].find(needle) {
            let start = offset + found + needle.len();
            let rest = text[start..].trim_start();
            let raw_words: Vec<&str> = rest.split_whitespace().take(6).collect();
            let mut total = chrono::Duration::zero();
            let mut i = 0;
            while i < raw_words.len() {
                let raw = raw_words[i];
                let ends_sentence = raw.ends_with(['.', ',', ')', ';', '!', '?']);
                let clean = raw.trim_end_matches(['.', ',', ')', ';', '!', '?']);
                if clean.is_empty() {
                    break;
                }
                if let Some(d) = crate::accounts::hub::cooldown::go_duration(clean) {
                    if d > chrono::Duration::zero() {
                        total += d;
                        i += 1;
                        if ends_sentence {
                            break;
                        }
                        continue;
                    }
                }
                if let Ok(value) = clean.parse::<i64>() {
                    if i + 1 < raw_words.len() {
                        let next_raw = raw_words[i + 1];
                        let next_ends = next_raw.ends_with(['.', ',', ')', ';', '!', '?']);
                        let next_clean = next_raw.trim_end_matches(['.', ',', ')', ';', '!', '?']);
                        if let Some(secs) = spaced_unit_seconds(value, next_clean) {
                            total += chrono::Duration::seconds(secs);
                            i += 2;
                            if next_ends {
                                break;
                            }
                            continue;
                        }
                    }
                }
                break;
            }
            if total > chrono::Duration::zero() {
                return Some(total);
            }
            offset = start;
        }
    }
    None
}

/*
CDXC:SessionChat 2026-08-21:
The picker as a notice. `detail` is Claude's OWN prose (the session's age and
token count, and its usage-limit recommendation) because that is the entire
basis for the choice — restating it in our words would drop the numbers the
user decides on. The switch-to-terminal action stays as the escape hatch for a
row this build cannot drive.
*/
fn notice_from_picker(
    screen: &NoticeScreen,
    picker: crate::session_chat_resume_prompt::SessionChatTerminalPicker,
) -> SessionChatTerminalNotice {
    use crate::session_chat_resume_prompt::SessionChatTerminalPickerKind;
    let picker = picker.with_catalog_model_names();
    // CDXC:SessionChat 2026-11-03 DECISION: User (Q1127): the card's rows already say "pick one", so the body is only the dialog's own words and the title is its question; the old "accepts no input until this is answered. Pick an option to answer it here." sentence is gone.
    let detail = picker.detail.clone().unwrap_or_default();
    let (kind, title) = match picker.kind {
        SessionChatTerminalPickerKind::Resume => (SESSION_CHAT_NOTICE_RESUME_PROMPT, "Resume how?"),
        SessionChatTerminalPickerKind::SwitchModel => {
            (SESSION_CHAT_NOTICE_SWITCH_CONFIRM_PROMPT, "Switch model?")
        }
        SessionChatTerminalPickerKind::SwitchEffort => {
            (SESSION_CHAT_NOTICE_SWITCH_CONFIRM_PROMPT, "Change effort?")
        }
        SessionChatTerminalPickerKind::SessionPaused => (
            SESSION_CHAT_NOTICE_SESSION_PAUSED_PROMPT,
            "Paused by safeguards",
        ),
        SessionChatTerminalPickerKind::PermissionPrompt
        | SessionChatTerminalPickerKind::AntigravityPermission
        | SessionChatTerminalPickerKind::CopilotPermission
        | SessionChatTerminalPickerKind::GrokPermission
        | SessionChatTerminalPickerKind::OmpPermission => {
            (SESSION_CHAT_NOTICE_PERMISSION_PROMPT, "Allow this?")
        }
    };
    SessionChatTerminalNotice::new(
        kind,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        title,
    )
    .with_detail(detail)
    .with_screen_tail(screen.screen_tail())
    .with_choices(
        picker
            .rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| SessionChatTerminalNoticeChoice {
                index,
                label: row.label,
                selected: row.selected,
            })
            .collect(),
    )
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_codex_blocking_screen(
    screen: &NoticeScreen,
    blocking: crate::session_chat_codex_blocking::CodexBlockingScreen,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        blocking.title,
    )
    .with_detail(blocking.detail)
    .with_screen_tail(screen.live_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_cursor_blocking_screen(
    screen: &NoticeScreen,
    blocking: crate::session_chat_cursor_blocking::CursorBlockingScreen,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_CURSOR_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        blocking.title,
    )
    .with_detail(blocking.detail)
    .with_screen_tail(screen.live_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_grok_blocking_screen(
    screen: &NoticeScreen,
    blocking: crate::session_chat_grok_blocking::GrokBlockingScreen,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_GROK_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        blocking.title,
    )
    .with_detail(blocking.detail)
    .with_screen_tail(screen.live_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_pi_blocking_screen(
    screen: &NoticeScreen,
    blocking: crate::session_chat_pi_blocking::PiBlockingScreen,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_PI_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        blocking.title,
    )
    .with_detail(blocking.detail)
    .with_screen_tail(screen.live_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_hermes_blocking_screen(
    screen: &NoticeScreen,
    blocking: crate::session_chat_hermes_blocking::HermesBlockingScreen,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        blocking.title,
    )
    .with_detail(blocking.detail)
    .with_screen_tail(screen.live_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_hermes_turn_error(
    screen: &NoticeScreen,
    error: String,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_AGENT_ERROR,
        SessionChatTerminalNoticeSeverity::Error,
        SessionChatTerminalNoticeSource::Screen,
        "Hermes could not finish the turn",
    )
    .with_input_blocking(false)
    .with_detail(error)
    .with_screen_tail(screen.screen_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

fn notice_from_omp_blocking_screen(
    screen: &NoticeScreen,
    blocking: crate::session_chat_omp_blocking::OmpBlockingScreen,
) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_OMP_INPUT_BLOCKED,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        blocking.title,
    )
    .with_detail(blocking.detail)
    .with_screen_tail(screen.live_tail())
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

/*
CDXC:SessionChat 2026-11-26 WHY:
W4 Q104 a: OMP's bash approvals are answered in the terminal, on the phone and in a coordinator,
because the screen cannot show whether a command ends in spaces. Until gxserver had read the
screen, the stored hook card reached the chat alone, and with no notice beside it the chat core
drew Allow and Deny, which gxserver then refused (`answerInTerminal`). So the card the screen
reading gives such an approval (`OMP_TOOL_APPROVAL`, Open terminal) is derived from the stored
bash hook card while the session shows no notice: a notice the session shows comes first, and
the derived one ends with the stored card. Its `detectedAt` names the stored card (a time taken
from the card's hash), so every producer of a frame restates the same instance, and the next card
is a new one.
SEE-ALSO: server/src/session_chat_omp_blocking.rs (`omp_hook_card_answered_in_terminal`).
*/
/// `notice`, or OMP's terminal card for the stored hook card `stored` of an OMP session (`agent`)
/// that is answered in the terminal, while there is no `notice`.
pub fn with_omp_hook_terminal_notice(
    agent: Option<&str>,
    stored: Option<&str>,
    notice: Option<SessionChatTerminalNotice>,
) -> Option<SessionChatTerminalNotice> {
    if notice.is_some() || session_chat_option_agent(agent) != Some(SessionChatOptionAgent::Omp) {
        return notice;
    }
    let stored = stored?;
    let Some(crate::session_chat_interactive::SessionChatInteractivePrompt::Approval {
        tool, ..
    }) = crate::session_chat::parse_stored_session_chat_prompt(stored)
    else {
        return None;
    };
    if !crate::session_chat_omp_blocking::omp_hook_card_answered_in_terminal(&tool) {
        return None;
    }
    let mut derived = notice_from_omp_blocking_screen(
        &NoticeScreen::new(""),
        crate::session_chat_omp_blocking::OMP_TOOL_APPROVAL,
    );
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(stored.as_bytes());
    let millis = digest[..5]
        .iter()
        .fold(0i64, |millis, byte| millis << 8 | i64::from(*byte));
    derived.detected_at = chrono::DateTime::from_timestamp_millis(millis)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    Some(derived)
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Pure classifier over ONE terminal capture. Rules are evaluated in the catalog's
precedence order (login > trust > permissions > exited > usage > stream >
update > onboarding), so the most blocking truth wins when a screen shows two.
`None` means "this screen is clean" — which is also what retires a notice, so
the tail windows above must stay tight enough that stale scrollback never keeps
one alive. Empryo has its own classifier and order (session_chat_empryo_blocking.rs).
*/
pub fn classify_session_chat_terminal_notice(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatTerminalNotice> {
    if crate::agents::identity::normalize_agent_id(agent).as_deref() == Some("empryo") {
        return crate::session_chat_empryo_blocking::classify_empryo_terminal_notice(screen_text);
    }
    let agent = session_chat_option_agent(agent)?;
    let screen = NoticeScreen::new(screen_text);
    if screen.folded.is_empty() {
        return None;
    }
    /*
    CDXC:AgentLauncher 2026-11-21 DECISION:
    User (Q4370): the PowerShell failure notice says which of its four error ids it saw, in plain words, instead of one message for all of them.
    */
    let powershell_error = screen
        .ends_with_shell_prompt()
        .then(|| {
            let tail = flatten_tail(&screen.folded, NOTICE_EXIT_SCAN_LINES);
            [
                (
                    "CommandNotFoundException",
                    "PowerShell couldn't find the agent's command (CommandNotFoundException): it isn't installed, or isn't on PATH. Install it in Settings > Agents, then start it again.",
                ),
                (
                    "UnauthorizedAccess",
                    "PowerShell blocked the agent's startup script (UnauthorizedAccess): the execution policy doesn't allow it to run. Check the error in the terminal.",
                ),
                (
                    "ParseException",
                    "PowerShell couldn't read the agent's startup command (ParseException). Check the agent's command in Settings > Agents and the error in the terminal.",
                ),
                (
                    "BadExpression",
                    "PowerShell couldn't run an expression in the agent's startup command (BadExpression). Check the agent's command in Settings > Agents and the error in the terminal.",
                ),
            ]
            .into_iter()
            .find(|(error, _)| {
                matches_parts(
                    &tail,
                    &[
                        NoticePart::Text("FullyQualifiedErrorId :"),
                        NoticePart::Gap(80),
                        NoticePart::Text(error),
                    ],
                )
            })
        })
        .flatten();
    if let Some((_, detail)) = powershell_error {
        return Some(
            SessionChatTerminalNotice::new(
                SESSION_CHAT_NOTICE_AGENT_EXITED,
                SessionChatTerminalNoticeSeverity::Error,
                SessionChatTerminalNoticeSource::Screen,
                "The agent failed to start",
            )
            .with_detail(detail)
            .with_screen_tail(screen.screen_tail())
            .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
                OPEN_TERMINAL.label,
            )]),
        );
    }
    if agent == SessionChatOptionAgent::Claude {
        if let Some(notice) = detect_claude_missing_login(&screen) {
            return Some(notice);
        }
    }
    if agent == SessionChatOptionAgent::Codex
        && crate::session_chat_codex_lock::is_locked(screen_text)
    {
        return Some(
            crate::session_chat_codex_lock::notice().with_screen_tail(screen.screen_tail()),
        );
    }
    if agent == SessionChatOptionAgent::Cursor {
        if let Some(notice) = crate::session_chat_cursor_login::detect_cursor_sign_in(screen_text) {
            return Some(notice);
        }
    }
    if let Some(notice) =
        crate::session_chat_workspace_trust::detect_workspace_trust_prompt(agent, screen_text)
    {
        return Some(notice);
    }
    /*
    CDXC:SessionChat 2026-08-21:
    The resume-usage picker outranks the whole catalog below it. Every rule
    there can only say "answer this in your terminal"; this one carries the
    rows, so the user answers it from the chat surface they are already on.
    Claude Code is the only CLI that paints it.
    */
    if agent == SessionChatOptionAgent::Claude {
        // An unattended prompt's countdown is cut out first (CDXC:SessionChat 2026-11-08 in
        // session_chat_claude_permission.rs), so a prompt stays one prompt while it ticks.
        let (screen_text, auto_block_in) =
            crate::session_chat_claude_permission::split_claude_auto_block(screen_text);
        let screen_text = screen_text.as_ref();
        if let Some(picker) =
            crate::session_chat_resume_prompt::detect_session_chat_terminal_picker(screen_text)
        {
            let permission = (picker.kind
                == crate::session_chat_resume_prompt::SessionChatTerminalPickerKind::PermissionPrompt)
                .then(|| {
                    crate::session_chat_claude_permission::detect_claude_permission_request(
                        screen_text,
                    )
                })
                .flatten();
            return Some(
                notice_from_picker(&screen, picker)
                    .with_permission_request(permission)
                    .with_auto_block_in(auto_block_in),
            );
        }
        if let Some(dialog) = crate::session_chat_claude_dialog::detect_claude_dialog(screen_text) {
            // CDXC:AgentProviders 2026-09-22 WHY:
            // Claude's limit-choice panel used to mask the quota error as a generic question, so automatic account switching never started. Keep its controls for manual use while exposing its actual recovery cause.
            if crate::session_chat_composer::is_claude_usage_limit_dialog(screen_text)
                || crate::session_chat_claude_dialog::is_claude_usage_limit_chooser(&dialog)
            {
                let mut notice = dialog
                    .into_notice(SESSION_CHAT_NOTICE_USAGE_LIMIT)
                    .with_input_blocking(true);
                notice.severity = SessionChatTerminalNoticeSeverity::Warning;
                return Some(notice);
            }
            let mut notice = dialog.into_notice(SESSION_CHAT_NOTICE_CLAUDE_INPUT_BLOCKED);
            // The terminal output opens on what Claude printed above the dialog (the chat core's
            // questions/notice_tail.rs), so the card carries the screen, not the dialog again.
            notice.screen_tail = screen.screen_tail();
            return Some(notice);
        }
    }
    if agent == SessionChatOptionAgent::Antigravity {
        if let Some(picker) =
            crate::session_chat_antigravity_permission::detect_antigravity_permission_picker(
                screen_text,
            )
        {
            return Some(notice_from_picker(&screen, picker));
        }
    }
    if agent == SessionChatOptionAgent::Copilot {
        if let Some(picker) =
            crate::session_chat_copilot_permission::detect_copilot_permission_picker(screen_text)
        {
            return Some(notice_from_picker(&screen, picker));
        }
    }
    if agent == SessionChatOptionAgent::Codex {
        if let Some(dialog) = crate::session_chat_codex_dialog::detect_codex_dialog(screen_text) {
            if dialog.is_codex_directory_trust() {
                let mut notice = dialog.into_notice(SESSION_CHAT_NOTICE_TRUST_PROMPT);
                notice.severity = SessionChatTerminalNoticeSeverity::Warning;
                notice.screen_tail = screen.screen_tail();
                return Some(notice);
            }
            let mut notice = dialog.into_notice(SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED);
            // CDXC:SessionChat 2026-10-27 DECISION: the terminal output opens on what Codex printed above its prompt (Q682, the chat core's questions/notice_tail.rs), so every Codex dialog card carries the screen rather than the dialog's own text again. An update prompt's output still shows the install command it would run.
            notice.screen_tail = screen.screen_tail();
            return Some(notice);
        }
    }
    // The agents whose dialogs replace their input box: a dialog phrase on a row above or inside
    // that agent's ready input box, or in the active-items list it draws under that box, is text
    // on the screen (`phrase_in_or_above_input_box_or_its_active_items`).
    let dialog_composer = match agent {
        SessionChatOptionAgent::Claude => Some("claude"),
        SessionChatOptionAgent::Antigravity => Some("antigravity"),
        _ => None,
    };
    let mut advisory_notice = None;
    for rule in notice_rules(agent) {
        if let Some(signature) = rule.signatures.iter().find(|signature| {
            if !signature_matches(&screen, signature) {
                return false;
            }
            if agent == SessionChatOptionAgent::Cursor
                && rule.kind == SESSION_CHAT_NOTICE_TRUST_PROMPT
            {
                // Cursor leaves the answered trust dialog above its new composer.
                let title_index = screen
                    .folded
                    .iter()
                    .rposition(|line| line.contains("Workspace Trust Required"));
                if title_index.is_some_and(|index| {
                    crate::session_chat_composer::detect_session_chat_composer_ready(
                        Some("cursor"),
                        &screen.display[index + 1..].join("\n"),
                    )
                    .state
                        == crate::session_chat_composer::SessionChatComposerState::Ready
                }) {
                    return false;
                }
            }
            if agent == SessionChatOptionAgent::Claude
                && rule.kind == SESSION_CHAT_NOTICE_LOGIN_EXPIRED
                && screen.has_claude_login_success_after(signature)
            {
                return false;
            }
            if agent == SessionChatOptionAgent::Claude
                && rule.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT
                && screen.has_claude_model_switch_after(signature)
            {
                return false;
            }
            // A limit chooser quoted in tool output or a message, with this session's own input
            // box below it, is another session's screen (`has_quoted_claude_limit_chooser`).
            if agent == SessionChatOptionAgent::Claude
                && rule.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT
                && signature_needle(signature)
                    .is_some_and(|needle| screen.has_quoted_claude_limit_chooser(needle))
            {
                return false;
            }
            if rule.blocks_input
                && matches!(signature.scope, NoticeScope::Dialog)
                && dialog_composer.is_some_and(|composer| {
                    screen.phrase_in_or_above_input_box_or_its_active_items(composer, signature)
                })
            {
                return false;
            }
            if agent != SessionChatOptionAgent::Codex
                || !matches!(
                    rule.kind,
                    SESSION_CHAT_NOTICE_UPDATE_PROMPT | SESSION_CHAT_NOTICE_TRUST_PROMPT
                )
            {
                return true;
            }
            !signature_needle(signature)
                .is_some_and(|needle| screen.has_codex_composer_after(needle))
        }) {
            let notice = notice_from_rule(&screen, rule, signature);
            if !notice.blocks_input() {
                // An inline error must not conceal a current dialog or picker.
                advisory_notice.get_or_insert(notice);
            } else {
                return Some(notice);
            }
        }
    }
    if agent == SessionChatOptionAgent::Codex {
        if let Some(blocking) =
            crate::session_chat_codex_blocking::detect_codex_blocking_screen(screen_text)
        {
            return Some(notice_from_codex_blocking_screen(&screen, blocking));
        }
    }
    if agent == SessionChatOptionAgent::Cursor {
        if let Some(notice) =
            crate::session_chat_cursor_decision::detect_cursor_decision_notice(screen_text)
        {
            return Some(notice);
        }
        if let Some(blocking) =
            crate::session_chat_cursor_blocking::detect_cursor_blocking_screen(screen_text)
        {
            return Some(notice_from_cursor_blocking_screen(&screen, blocking));
        }
    }
    if agent == SessionChatOptionAgent::Grok {
        if let Some(notice) =
            crate::session_chat_grok_blocking::detect_grok_trust_prompt(screen_text)
        {
            return Some(notice);
        }
        if let Some(picker) =
            crate::session_chat_grok_permission::detect_grok_permission_picker(screen_text)
        {
            return Some(notice_from_picker(&screen, picker).with_approval_command(
                crate::session_chat_grok_permission::grok_permission_command(screen_text),
            ));
        }
        if let Some(blocking) =
            crate::session_chat_grok_blocking::detect_grok_blocking_screen(screen_text)
        {
            return Some(notice_from_grok_blocking_screen(&screen, blocking));
        }
    }
    if agent == SessionChatOptionAgent::Hermes {
        if let Some(notice) =
            crate::session_chat_hermes_blocking::detect_hermes_hook_trust(screen_text)
        {
            return Some(notice);
        }
        if let Some(blocking) =
            crate::session_chat_hermes_blocking::detect_hermes_blocking_screen(screen_text)
        {
            return Some(notice_from_hermes_blocking_screen(&screen, blocking));
        }
        if let Some(error) =
            crate::session_chat_hermes_blocking::detect_hermes_turn_error(screen_text)
        {
            return Some(notice_from_hermes_turn_error(&screen, error));
        }
    }
    if agent == SessionChatOptionAgent::Omp {
        if let Some((picker, command)) =
            crate::session_chat_omp_blocking::detect_omp_permission_picker_with_command(screen_text)
        {
            return Some(notice_from_picker(&screen, picker).with_approval_command(command));
        }
        if let Some(blocking) =
            crate::session_chat_omp_blocking::detect_omp_blocking_screen(screen_text)
        {
            return Some(notice_from_omp_blocking_screen(&screen, blocking));
        }
    }
    if agent == SessionChatOptionAgent::Pi {
        if let Some(notice) = crate::session_chat_pi_blocking::detect_pi_trust_prompt(screen_text) {
            return Some(notice);
        }
        if let Some(blocking) =
            crate::session_chat_pi_blocking::detect_pi_blocking_screen(screen_text)
        {
            return Some(notice_from_pi_blocking_screen(&screen, blocking));
        }
    }
    advisory_notice
}

/*
CDXC:AgentScreenDetection 2026-08-19:
Codex queues input typed while a turn runs CLIENT-SIDE: nothing is written to
the rollout until the turn ends. The send watchdog must consult this before it
declares a message undelivered, which is why the state is exposed as a
predicate instead of as a user-facing notice.
*/
/// CDXC:AgentScreenDetection 2026-09-11 DECISION:
/// User: scan the whole terminal screen for Codex's queued-message indicator, so long queued messages cannot hide it from the delivery watchdog.
pub fn session_chat_screen_shows_queued_input(
    agent: Option<&str>,
    screen_text: &str,
    sent_text: &str,
) -> bool {
    if session_chat_option_agent(agent) != Some(SessionChatOptionAgent::Codex) {
        return false;
    }
    let needle = sent_text.split_whitespace().collect::<Vec<_>>().join(" ");
    !needle.is_empty()
        && codex_queued_input_previews(screen_text)
            .iter()
            .any(|preview| {
                needle == *preview || (preview.chars().count() >= 12 && needle.starts_with(preview))
            })
}

/// CDXC:SessionChat 2026-09-18 WHY:
/// Accepted async answers stay in Codex's client-side queue during compaction, before any UserMessage reaches the transcript.
/// Both queue headings carry the same outgoing previews; the heading alone also appears with unanswered questions and proves nothing.
pub(crate) fn codex_queued_input_previews(screen_text: &str) -> Vec<String> {
    let lines: Vec<String> = strip_ansi_sgr(screen_text)
        .lines()
        .map(normalize_spaces)
        .collect();
    let Some(header) = lines.iter().rposition(|line| {
        matches!(
            line.trim(),
            "• Queued follow-up inputs" | "• Queued followup inputs"
        ) || line
            .trim()
            .starts_with("• Messages to be submitted after next tool call (")
    }) else {
        return Vec::new();
    };
    let mut previews = Vec::new();
    let mut preview = String::new();
    for line in &lines[header + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(first) = trimmed.strip_prefix("↳ ") {
            if !preview.is_empty() {
                previews.push(std::mem::take(&mut preview));
            }
            preview = first.split_whitespace().collect::<Vec<_>>().join(" ");
        } else if trimmed == "…" && !preview.is_empty() {
            previews.push(std::mem::take(&mut preview));
        } else if !preview.is_empty()
            && line.starts_with("    ")
            && !trimmed.contains("edit last queued message")
        {
            preview.push(' ');
            preview.push_str(&trimmed.split_whitespace().collect::<Vec<_>>().join(" "));
        } else if preview.is_empty() && trimmed.ends_with("and send immediately)") {
            // The long queue heading wraps in a narrow terminal.
            continue;
        } else {
            break;
        }
    }
    if !preview.is_empty() {
        previews.push(preview);
    }
    previews
}

/// The trimmed screen tail a watchdog notice attaches as evidence.
pub fn session_chat_terminal_screen_tail(screen_text: &str) -> Option<String> {
    NoticeScreen::new(screen_text).screen_tail()
}

/*
CDXC:AgentScreenDetection 2026-08-24:
The one delivery verdict that is not reasoning from silence: the agent recorded
a user turn AFTER the send that is not the message we sent — normally an EMPTY
one, because the send's trailing Enter submitted the composer before the paste
had been ingested into it. The agent then answers that empty turn, which is why
this case used to be swallowed by the watchdog's "already working, still
working" suppression: the working turn is the SYMPTOM, not proof of delivery.

The wording lives here, with the rest of the catalog, and says the two things
the user cannot see from chat: the message was not delivered, and where the text
went. It stays in the terminal's composer until the user opens Terminal view or
a later Chat send deliberately clears that hidden input before pasting.
*/
pub fn session_chat_delivery_mismatch_notice(
    submitted_empty: bool,
    screen_tail: Option<String>,
) -> SessionChatTerminalNotice {
    let recorded = if submitted_empty {
        "an empty message"
    } else {
        "a different message"
    };
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_DELIVERY_FAILED,
        SessionChatTerminalNoticeSeverity::Error,
        SessionChatTerminalNoticeSource::Watchdog,
        "Your message didn't arrive",
    )
    .with_detail(format!(
        "The agent got {recorded} instead and answered that. Your text may still be in the terminal's input box."
    ))
    .with_screen_tail(screen_tail)
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}

/*
CDXC:AgentScreenDetection 2026-08-28:
The transcript recorded an API refusal row for the last turn (see
`claude_api_refusal_text`). The detail is the CLI's own recorded explanation
verbatim — it already names the safeguards, the category tag, the model-switch
escape hatch and the request id, and paraphrasing it would only lose the parts
support asks for.
*/
pub fn session_chat_api_refusal_notice(recorded_text: String) -> SessionChatTerminalNotice {
    SessionChatTerminalNotice::new(
        SESSION_CHAT_NOTICE_API_REFUSAL,
        SessionChatTerminalNoticeSeverity::Error,
        SessionChatTerminalNoticeSource::Watchdog,
        "The agent could not respond to this message",
    )
    .with_detail(recorded_text)
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        OPEN_TERMINAL.label,
    )])
}
