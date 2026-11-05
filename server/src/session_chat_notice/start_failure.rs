use super::*;

/// The id of the card's Sign in action. It rides the `restartAgent` answer every client already
/// sends, with this id as `actionId` (session_chat_agent_restart.rs), so a client built before the
/// action existed still restarts the agent instead of doing nothing.
pub const SESSION_CHAT_NOTICE_ACTION_SIGN_IN: &str = "signInAccount";
/// The id of Reconnect on the card of an agent that reported a sign-in error while it kept running
/// (session_chat_account_sign_in.rs). It rides the `restartAgent` answer too.
pub const SESSION_CHAT_NOTICE_ACTION_RECONNECT: &str = "reconnectAccount";
/// The id of the card's Try again action, the same restart the Restart action of an exited agent is.
pub const SESSION_CHAT_NOTICE_ACTION_RETRY_START: &str = "retryStart";

/// What claude-swap prints when `cswap run <slot>` finds no saved login for that slot.
const CSWAP_NO_CREDENTIALS: &str = "has no stored credentials";

/// CDXC:AgentProviders 2026-10-14 WHY:
/// Waking a session on a cswap account with no saved login printed "Error: Account-5 has no stored credentials", the restore script's fallback failed the same way, and the terminal dropped to the shell; nothing classified that screen, so gxserver kept the session running and idle, the chat showed nothing, and the message the user had sent waited as a startup send for good. The screen now becomes a sign-in card that names the account and offers Switch account (every `loginExpired` card has it), Sign in to that account and Try again. Waiting cannot bring a login back, so the queue holds the message behind this card instead of retrying it, and sends it once the agent is up (session_chat_agent_restart.rs heals the start when a working account or the login appears).
/// SEE-ALSO: accounts/wake_account.rs (a wake avoids such an account when it may switch), session_chat_queue_runtime/scheduler.rs (holds rows behind the card).
pub(super) fn detect_claude_missing_login(
    screen: &NoticeScreen,
) -> Option<SessionChatTerminalNotice> {
    if !screen.ends_with_shell_prompt() {
        return None;
    }
    let slot = cswap_missing_login_slot(&flatten_tail(&screen.folded, NOTICE_EXIT_SCAN_LINES))?;
    let account = card_account_name(&slot, claude_slot_email(&slot).as_deref());
    Some(
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_LOGIN_EXPIRED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Screen,
            format!("{account} isn't signed in here"),
        )
        .with_input_blocking(true)
        .with_detail(format!(
            "Sign in to {account} on this computer, or switch this chat to another account. Messages you send wait until Claude starts."
        ))
        .with_screen_tail(screen.screen_tail())
        .with_actions(vec![
            SessionChatTerminalNoticeAction {
                id: SESSION_CHAT_NOTICE_ACTION_SIGN_IN.to_string(),
                label: "Sign in".to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            SessionChatTerminalNoticeAction {
                id: SESSION_CHAT_NOTICE_ACTION_RETRY_START.to_string(),
                label: "Try again".to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            SessionChatTerminalNoticeAction::switch_to_terminal(OPEN_TERMINAL.label),
        ]),
    )
}

/// The card for an agent whose start command ended before the agent came up, when `screen_text`
/// is back at a shell prompt. Only the screen half: the session's process tree and its missing
/// conversation, which make it a start failure, are checked by session_chat_start_failure.rs.
pub fn agent_start_failure_notice(screen_text: &str) -> Option<SessionChatTerminalNotice> {
    let screen = NoticeScreen::new(screen_text);
    if !screen.ends_with_any_shell_prompt() {
        return None;
    }
    let screen_tail = screen.screen_tail();
    let error_lines = start_failure_error_lines(screen_text);
    Some(
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_AGENT_EXITED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Screen,
            "The agent couldn't start",
        )
        .with_input_blocking(true)
        .with_detail(START_FAILURE_DETAIL)
        .with_error_lines(error_lines)
        .with_screen_tail(screen_tail)
        .with_actions(vec![
            SessionChatTerminalNoticeAction {
                id: SESSION_CHAT_NOTICE_ACTION_RETRY_START.to_string(),
                label: "Try again".to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
            SessionChatTerminalNoticeAction::switch_to_terminal(OPEN_TERMINAL.label),
        ]),
    )
}

/// What a failed start's card says; its error lines sit behind the card's Details toggle.
pub const START_FAILURE_DETAIL: &str =
    "It stopped with an error. Fix it and try again; messages you send wait until it starts.";

/// How a card names a saved account: its email, or "Account N" while Settings hides account emails
/// (`hideAccountEmails`) or no email is known.
/// CDXC:AgentProviders 2026-11-03 DECISION: User (Q1135): a card names the account by its email ("maddada123spam@gmail.com is signed out"); "watch out we need to hide email if it's hidden in settings", so with Hide account emails on it says "Account 2", never the email. gxserver picks the name, so the desktop, the web build and the phone all get the same words.
/// SEE-ALSO: server/src/session_chat_account_sign_in.rs, packages/settings-catalog/src/data/defaults.rs (`hideAccountEmails`).
pub(crate) fn card_account_name(selector: &str, email: Option<&str>) -> String {
    let email = email.map(str::trim).filter(|email| !email.is_empty());
    match email {
        Some(email) if !account_emails_hidden() => email.to_string(),
        _ => format!("Account {selector}"),
    }
}

/// Settings > Accounts > Hide account emails, read fresh from the shared settings file.
fn account_emails_hidden() -> bool {
    crate::shared_settings_file::read_shared_settings(&crate::paths::get_gxserver_paths(None))
        .and_then(|settings| settings.get("hideAccountEmails").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// The id of Install on the card of an agent whose command is not on this computer. It rides the
/// `restartAgent` answer every client already sends (session_chat_agent_restart.rs).
pub const SESSION_CHAT_NOTICE_ACTION_INSTALL_AGENT: &str = "installAgentCli";

/// The program a shell said it could not find, from the lines a failed start printed: `env`'s
/// "No such file or directory", zsh's and bash's "command not found", fish's "Unknown command" and
/// PowerShell's "is not recognized". A path keeps only its last part.
pub fn missing_start_command(error_lines: &str) -> Option<String> {
    let name = |text: &str| -> Option<String> {
        let text = text.trim().trim_matches(['\'', '"', '`']);
        let name = text.rsplit(['/', '\\']).next().unwrap_or(text);
        (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| name.to_string())
    };
    error_lines.lines().rev().find_map(|line| {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("env: ") {
            return name(rest.strip_suffix(": No such file or directory")?);
        }
        if let Some(rest) = line.strip_prefix("zsh: command not found: ") {
            return name(rest);
        }
        if let Some(rest) = line.strip_prefix("zsh: no such file or directory: ") {
            return name(rest);
        }
        if let Some(rest) = line.strip_prefix("fish: Unknown command: ") {
            return name(rest);
        }
        if let Some(rest) = line.strip_suffix(": command not found") {
            // `bash: dsh-tui: command not found`, `sh: 1: dsh-tui: command not found`
            return name(rest.rsplit(": ").next()?);
        }
        let start = line.find("The term '")? + "The term '".len();
        let end = start + line[start..].find('\'')?;
        line[end..]
            .contains("is not recognized")
            .then(|| name(&line[start..end]))
            .flatten()
    })
}

/// CDXC:SessionChat 2026-10-28 WHY:
/// A DeepSeek Harness session's card read only `env: dsh-tui: No such file or directory`, and the person who hit it had to ask another agent what it meant. When a failed start says the agent's own command is missing, the card says so in words and offers Install (the method Settings > Agents would use, then a restart), or, when Ghostex does find the command somewhere this terminal does not look, says where it is.
/// `installed_at` is where gxserver finds the command, if anywhere.
pub fn agent_command_missing_notice(
    mut notice: SessionChatTerminalNotice,
    agent_name: &str,
    command: &str,
    installed_at: Option<&str>,
) -> SessionChatTerminalNotice {
    match installed_at {
        None => {
            notice.title = format!("{agent_name} isn't installed");
            notice.detail = Some(
                "Install it to start chatting. Messages you send wait until it's running."
                    .to_string(),
            );
            notice.actions = vec![
                SessionChatTerminalNoticeAction {
                    id: SESSION_CHAT_NOTICE_ACTION_INSTALL_AGENT.to_string(),
                    label: format!("Install {agent_name}"),
                    kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                    send: None,
                },
                SessionChatTerminalNoticeAction::switch_to_terminal(OPEN_TERMINAL.label),
            ];
        }
        Some(path) => {
            notice.title = format!("This terminal can't find {command}");
            notice.detail = Some(format!(
                "{agent_name} is installed at {path}, but the terminal doesn't look there. Use Add to PATH in Settings > Agents, then try again."
            ));
        }
    }
    notice
}

/// Lines the card shows for a failed start.
const START_FAILURE_ERROR_LINES: usize = 4;
/// Longest piece of one of those lines the card shows, from its start.
const START_FAILURE_LINE_CHARS: usize = 300;
/// Narrowest screen whose full rows are read as wrapped lines.
const WRAPPED_ROW_MIN_WIDTH: usize = 60;

/// CDXC:SessionChat 2026-10-16 WHY:
/// The failed-start card has to say why without the user opening the terminal: a customer's card would have read "--share-history is not supported on Windows yet" had it shown cswap's own words. These are the last block of output above the shell prompt the failed start left (the prompt line dropped, at most four lines, stopping at a blank line so older output stays out), shown behind the card's Details toggle since 2026-11-03 (Q1136).
/// CDXC:SessionChat 2026-11-05 WHY:
/// GitHub #422: Cursor refused its launch model with one line, "Cannot use this model: Auto Balance. Available models: auto, …", six thousand characters that fill the screen as wrapped rows, so the last four rows were the middle of the model list and the reason was never on the card. Rows as wide as the screen's widest row (or one cell short, where the wrap fell after a space the capture trims) are joined to the row under them, each line is cut at 300 characters from its start, and the lines come from the whole capture, not the notice's tail, which keeps only its last 6000 characters.
/// SEE-ALSO: apps/desktop/src/app/native_chat/notice.rs and apps/mobile/app/src/chat/native/cards/NoticeCard.tsx draw `errorLines`.
fn start_failure_error_lines(screen_text: &str) -> Option<String> {
    let mut rows: Vec<String> = screen_text
        .lines()
        .map(|row| {
            normalize_spaces(&strip_ansi_sgr(row))
                .trim_end()
                .to_string()
        })
        .collect();
    while rows.last().is_some_and(|row| row.trim().is_empty()) {
        rows.pop();
    }
    rows.pop();
    let width = rows.iter().map(|row| row.chars().count()).max()?;
    let block_start = rows
        .iter()
        .rposition(|row| row.trim().is_empty())
        .map_or(0, |blank| blank + 1);
    let mut lines: Vec<String> = Vec::new();
    // Set while the row above filled the screen: `Some(true)` when it ended on a space, which the
    // capture trims, so it is one cell short.
    let mut continues: Option<bool> = None;
    for row in &rows[block_start..] {
        match (lines.last_mut(), continues) {
            (Some(line), Some(trimmed_space)) => {
                if trimmed_space {
                    line.push(' ');
                }
                line.push_str(row);
            }
            _ => lines.push(row.clone()),
        }
        let cells = row.chars().count();
        continues = (width >= WRAPPED_ROW_MIN_WIDTH && cells + 1 >= width).then_some(cells < width);
    }
    let shown: Vec<String> = lines[lines.len().saturating_sub(START_FAILURE_ERROR_LINES)..]
        .iter()
        .map(
            |line| match line.char_indices().nth(START_FAILURE_LINE_CHARS) {
                Some((cut, _)) => format!("{}\u{2026}", &line[..cut]),
                None => line.clone(),
            },
        )
        .collect();
    (!shown.is_empty()).then(|| shown.join("\n"))
}

/// The last email seen for each Claude cswap slot.
static CLAUDE_SLOT_EMAILS: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

/// The email of Claude cswap `slot`, from the shared account snapshot or, while that has no row
/// for it, the last one seen.
/// WHY: a cswap read that fails leaves the snapshot without its Claude rows for a couple of
/// minutes, and a title without the email made the card a new instance, which brought back a
/// card the user had dismissed.
pub(crate) fn claude_slot_email(slot: &str) -> Option<String> {
    let current = crate::accounts::runtime::current_snapshot()
        .accounts
        .into_iter()
        .find(|account| {
            account.provider == crate::accounts::model::Provider::Claude && account.selector == slot
        })
        .map(|account| account.email)
        .filter(|email| !email.is_empty());
    let mut emails = CLAUDE_SLOT_EMAILS.lock().ok()?;
    match current {
        Some(email) => {
            emails.insert(slot.to_string(), email.clone());
            Some(email)
        }
        None => emails.get(slot).cloned(),
    }
}

/// The cswap slot named by the last "Account-<slot> has no stored credentials" in `text`.
pub fn cswap_missing_login_slot(text: &str) -> Option<String> {
    let before = &text[..text.rfind(CSWAP_NO_CREDENTIALS)?];
    let after_label = &before[before.rfind("Account-")? + "Account-".len()..];
    // A narrow terminal can wrap the line right after the hyphen.
    let slot = after_label.trim();
    (!slot.is_empty() && slot.chars().all(|ch| ch.is_ascii_digit())).then(|| slot.to_string())
}

impl SessionChatTerminalNotice {
    /// The agent could not start because its account has no saved login on this computer
    /// (`detect_claude_missing_login`): only the user, or a switch to another account, fixes it.
    pub fn missing_login(&self) -> bool {
        self.kind == SESSION_CHAT_NOTICE_LOGIN_EXPIRED
            && self
                .actions
                .iter()
                .any(|action| action.id == SESSION_CHAT_NOTICE_ACTION_SIGN_IN)
    }

    /// Claude or Codex reported that a request could not sign in ("Login expired · Please run
    /// /login") while the agent kept running: not a disabled organization, a missing saved login,
    /// or a sign-in screen waiting in the terminal, which all block input or have their own card.
    pub fn sign_in_error(&self) -> bool {
        self.kind == SESSION_CHAT_NOTICE_LOGIN_EXPIRED
            && !self.blocks_input()
            && !self.account_disabled()
            && !self.missing_login()
    }

    /// The agent's start command ended before the agent came up (`agent_start_failure_notice`):
    /// restarting it on its own would fail the same way, so the card waits for the user.
    pub fn start_failed(&self) -> bool {
        self.kind == SESSION_CHAT_NOTICE_AGENT_EXITED
            && self
                .actions
                .iter()
                .any(|action| action.id == SESSION_CHAT_NOTICE_ACTION_RETRY_START)
    }

    /// The cswap slot a `missing_login` card is about, read back from its screen tail.
    pub fn missing_login_slot(&self) -> Option<String> {
        self.missing_login()
            .then(|| cswap_missing_login_slot(&self.screen_tail.as_deref()?.replace('\n', " ")))
            .flatten()
    }
}

/// Whether the newest non-blank line of `screen_text` is a bare shell prompt, so nothing (no agent,
/// no login) runs in front of the shell.
pub fn session_chat_screen_ends_with_shell_prompt(screen_text: &str) -> bool {
    NoticeScreen::new(screen_text).ends_with_shell_prompt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Claude screen whose last turn failed with `line`, back at its input box.
    fn claude_screen(line: &str) -> String {
        format!(
            "> fix the login check\n\n  ⎿  {line}\n\n╭────────────────────────────────────────╮\n│ >                                      │\n╰────────────────────────────────────────╯\n  ? for shortcuts\n"
        )
    }

    #[test]
    fn a_login_error_while_claude_runs_is_a_sign_in_error() {
        let notice = classify_session_chat_terminal_notice(
            Some("claude"),
            &claude_screen("Login expired · Please run /login"),
        )
        .expect("the login error is classified");
        assert_eq!(notice.kind, SESSION_CHAT_NOTICE_LOGIN_EXPIRED);
        assert!(notice.sign_in_error());
    }

    #[test]
    fn a_disabled_organization_or_a_missing_saved_login_is_not() {
        let disabled = classify_session_chat_terminal_notice(
            Some("claude"),
            &claude_screen(
                "API Error: 403 Your organization has disabled Claude subscription access for Claude Code",
            ),
        )
        .expect("the refusal is classified");
        assert!(disabled.account_disabled());
        assert!(!disabled.sign_in_error());
        let missing = classify_session_chat_terminal_notice(
            Some("claude"),
            "Error: Account-5 has no stored credentials. Re-add with: cswap --add-account --slot 5\nuser@mac ~ % ",
        )
        .expect("the failed start is classified");
        assert!(missing.missing_login());
        assert!(!missing.sign_in_error());
    }

    /// GitHub #422: Cursor 2026.11.05 refusing the "Auto Balance" name Ghostex had pinned as its
    /// model. Captured live on 2026-11-05 and wrapped at the reporter's 200 columns: one line that
    /// fills the screen above the shell prompt.
    #[test]
    fn cursor_refusing_its_launch_model_is_a_failed_start_that_says_why() {
        let screen = include_str!("../../tests/fixtures/cursor-unknown-model-start-failure.txt");
        assert!(classify_session_chat_terminal_notice(Some("cursor"), screen).is_none());
        let notice = agent_start_failure_notice(screen).expect("a failed start");
        assert!(notice.start_failed());
        let lines = notice.error_lines.expect("the agent's words");
        assert!(
            lines.starts_with(
                "Cannot use this model: Auto Balance. Available models: auto, gpt-5.3-codex-low,"
            ),
            "{lines}"
        );
        assert!(lines.ends_with('\u{2026}'), "{lines}");
        assert_eq!(lines.lines().count(), 1, "{lines}");
    }

    /// The same failed start under prompts the bare-prompt test missed: oh-my-zsh's default theme
    /// and a prompt with a right-hand part.
    #[test]
    fn a_failed_start_is_seen_under_any_common_prompt() {
        let screen = include_str!("../../tests/fixtures/cursor-unknown-model-start-failure.txt");
        let error = &screen[..screen.rfind("madda@").expect("the fixture's prompt")];
        for prompt in [
            "\u{279c}  scratch git:(main) \u{2717}",
            "~/dev/scratch \u{276f}                                         \u{2714}  10:42:07",
            "madda@mac scratch % ",
        ] {
            let notice = agent_start_failure_notice(&format!("{error}{prompt}\n"))
                .unwrap_or_else(|| panic!("no failed start under {prompt:?}"));
            assert!(notice
                .error_lines
                .is_some_and(|lines| lines.starts_with("Cannot use this model: Auto Balance.")));
        }
        // A running agent's own rows are not prompts.
        for row in [
            "  \u{2192} Plan, search, build anything",
            "  New Agent \u{b7} Auto Balance \u{b7} 0 used",
        ] {
            assert!(
                agent_start_failure_notice(&format!("{error}{row}\n")).is_none(),
                "{row}"
            );
        }
    }

    #[test]
    fn short_error_lines_are_shown_as_they_are() {
        let notice = agent_start_failure_notice(
            "PS C:\\Users\\me> cswap run 3 --share-history -- claude\n--share-history is not supported on Windows yet\nPS C:\\Users\\me>",
        )
        .expect("a failed start");
        assert_eq!(
            notice.error_lines.as_deref(),
            Some("PS C:\\Users\\me> cswap run 3 --share-history -- claude\n--share-history is not supported on Windows yet")
        );
    }
}

#[cfg(test)]
mod missing_command_tests {
    use super::*;

    #[test]
    fn every_shell_names_the_command_it_could_not_find() {
        for (lines, expected) in [
            ("env: dsh-tui: No such file or directory", "dsh-tui"),
            ("zsh: command not found: dsh-tui", "dsh-tui"),
            ("bash: dsh-tui: command not found", "dsh-tui"),
            ("sh: 1: gemini: command not found", "gemini"),
            ("fish: Unknown command: grok", "grok"),
            (
                "zsh: no such file or directory: /Users/u/.local/share/mise/shims/dsh-tui",
                "dsh-tui",
            ),
            (
                "dsh-tui : The term 'dsh-tui' is not recognized as a name of a cmdlet, function, script file, or executable program.",
                "dsh-tui",
            ),
        ] {
            assert_eq!(
                missing_start_command(lines).as_deref(),
                Some(expected),
                "{lines}"
            );
        }
        assert_eq!(missing_start_command("Error: invalid flag --yolo"), None);
    }

    #[test]
    fn a_missing_agent_card_offers_install_or_says_where_it_is() {
        let base = agent_start_failure_notice(
            "$ env DSH_TUI_NO_LAUNCHPAD=1 dsh-tui\nenv: dsh-tui: No such file or directory\nuser@mac ~ % ",
        )
        .expect("a failed start");
        let missing =
            agent_command_missing_notice(base.clone(), "DeepSeek Harness", "dsh-tui", None);
        assert_eq!(missing.title, "DeepSeek Harness isn't installed");
        assert_eq!(
            missing.actions[0].id,
            SESSION_CHAT_NOTICE_ACTION_INSTALL_AGENT
        );
        assert_eq!(
            missing.actions[0].kind,
            SessionChatTerminalNoticeActionKind::RestartAgent
        );
        let elsewhere = agent_command_missing_notice(
            base,
            "DeepSeek Harness",
            "dsh-tui",
            Some("/opt/tools/dsh-tui"),
        );
        assert_eq!(elsewhere.title, "This terminal can't find dsh-tui");
        assert!(elsewhere
            .detail
            .unwrap()
            .contains("installed at /opt/tools/dsh-tui"));
        assert!(elsewhere
            .actions
            .iter()
            .any(|action| action.id == SESSION_CHAT_NOTICE_ACTION_RETRY_START));
    }
}
