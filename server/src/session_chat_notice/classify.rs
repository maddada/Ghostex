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
    .with_detail(match evidence.as_deref().and_then(limit_reset) {
        Some(reset) if rule.detail == CLAUDE_USAGE_LIMIT_DETAIL => {
            rule.detail
                .replacen("usage limit.", &format!("usage limit ({reset})."), 1)
        }
        _ => rule.detail.to_string(),
    })
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

/// "resets 4pm" from Claude's "You've hit your limit · resets 4pm (Europe/Berlin)", without the time zone.
fn limit_reset(line: &str) -> Option<String> {
    let reset = &line[line.find("resets ")?..];
    let reset = match reset.find(" (") {
        Some(at) => &reset[..at],
        None => reset,
    };
    let reset = reset.trim().trim_end_matches(['.', ',']);
    (reset.len() > "resets ".len()).then(|| reset.to_string())
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
    if screen.ends_with_shell_prompt()
        && [
            "ParseException",
            "BadExpression",
            "CommandNotFoundException",
            "UnauthorizedAccess",
        ]
        .iter()
        .any(|error| {
            matches_parts(
                &flatten_tail(&screen.folded, NOTICE_EXIT_SCAN_LINES),
                &[
                    NoticePart::Text("FullyQualifiedErrorId :"),
                    NoticePart::Gap(80),
                    NoticePart::Text(error),
                ],
            )
        })
    {
        return Some(
            SessionChatTerminalNotice::new(
                SESSION_CHAT_NOTICE_AGENT_EXITED,
                SessionChatTerminalNoticeSeverity::Error,
                SessionChatTerminalNoticeSource::Screen,
                "The agent failed to start",
            )
            .with_detail("PowerShell could not run the agent's startup command. Check the error in the terminal before starting it again.")
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
            return Some(notice_from_picker(&screen, picker).with_permission_request(permission));
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
            if agent == SessionChatOptionAgent::Claude
                && rule.blocks_input
                && matches!(signature.scope, NoticeScope::Dialog)
                && signature_needle(signature)
                    .is_some_and(|needle| screen.has_claude_composer_after(needle))
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
        if let Some(picker) =
            crate::session_chat_omp_blocking::detect_omp_permission_picker(screen_text)
        {
            return Some(notice_from_picker(&screen, picker));
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
