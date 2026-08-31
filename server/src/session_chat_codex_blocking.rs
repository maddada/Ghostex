/*
CDXC:SessionChatCodexBlockingScreens 2026-08-31:
Codex TUI audit against openai/codex a9519cbc. The TUI has two complementary
ways to say that ordinary composer input is unavailable:

  * agent-owned request views mark `terminal_title_requires_action()` (command,
    patch, permission and MCP approvals; request_user_input; MCP elicitation
    forms; app/tool suggestions);
  * every other chooser temporarily replaces the composer with the shared
    numbered selection UI (model, effort, permissions, startup hook review,
    auth/Bedrock, Windows sandbox, migrations, archived-thread recovery and
    the remaining slash-command pickers).

Ghostex already projects Codex `request_user_input` as a structured question
card, so this detector deliberately leaves that richer path alone. Everything
else is classified from the current terminal screen. A generic live-numbered-
choice rule is the exhaustive part: it follows Codex's shared selection row
shape instead of restating every call site, while the named cases below provide
useful guidance for the choices most likely to appear without the user opening
a slash menu themselves.

Liveness is positive. A selected numbered row must be visible and no later
Codex composer row may exist. This prevents an answered chooser left in
scrollback from keeping chat blocked after the real composer has returned.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const CODEX_BLOCKING_SCAN_LINES: usize = 120;
const SELECTION_MARKERS: &[char] = &['\u{203a}', '\u{276f}', '\u{25b6}', '\u{2192}', '>'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodexBlockingScreen {
    pub title: &'static str,
    pub detail: &'static str,
}

fn collapse_spaces(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut pending_space = false;
    for character in line.chars() {
        if character == ' ' {
            pending_space = !output.is_empty();
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.push(character);
    }
    output
}

fn scan_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines().rev() {
        let line = collapse_spaces(normalize_spaces(&strip_ansi_sgr(raw)).trim_end().trim());
        if line.is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= CODEX_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn strip_box_border(line: &str) -> &str {
    const BORDERS: &[char] = &['\u{2502}', '\u{2503}', '\u{2506}', '\u{250a}', '|'];
    line.trim()
        .trim_start_matches(BORDERS)
        .trim_end_matches(BORDERS)
        .trim()
}

fn is_codex_composer_line(line: &str) -> bool {
    let Some(rest) = strip_box_border(line).strip_prefix('\u{203a}') else {
        return false;
    };
    let rest = rest.trim_start();
    let digit_count = rest.chars().take_while(char::is_ascii_digit).count();
    if digit_count == 0 {
        return true;
    }
    !matches!(rest.chars().nth(digit_count), Some('.' | ')'))
}

fn is_selected_numbered_choice(line: &str) -> bool {
    let line = strip_box_border(line);
    let Some(marker) = line.chars().next() else {
        return false;
    };
    if !SELECTION_MARKERS.contains(&marker) {
        return false;
    }
    let rest = line[marker.len_utf8()..].trim_start();
    let digit_count = rest.chars().take_while(char::is_ascii_digit).count();
    digit_count > 0 && matches!(rest.chars().nth(digit_count), Some('.' | ')'))
}

fn composer_after(lines: &[String], index: usize) -> bool {
    lines
        .iter()
        .skip(index + 1)
        .any(|line| is_codex_composer_line(line))
}

fn latest_line_containing(lines: &[String], needles: &[&str]) -> Option<usize> {
    lines.iter().rposition(|line| {
        let line = line.to_ascii_lowercase();
        needles
            .iter()
            .any(|needle| line.contains(&needle.to_ascii_lowercase()))
    })
}

fn latest_selected_choice(lines: &[String]) -> Option<usize> {
    lines
        .iter()
        .rposition(|line| is_selected_numbered_choice(line))
}

fn latest_modal_footer(lines: &[String]) -> Option<usize> {
    lines.iter().rposition(|line| {
        let line = line.to_ascii_lowercase();
        let dismisses =
            line.contains("go back") || line.contains("cancel") || line.contains("close");
        let accepts = line.contains("confirm") || line.contains("select");
        dismisses && (line.contains("esc") || (line.contains("press") && accepts))
    })
}

fn live_named_screen(
    lines: &[String],
    needles: &[&str],
    title: &'static str,
    detail: &'static str,
) -> Option<CodexBlockingScreen> {
    let index = latest_line_containing(lines, needles)?;
    (!composer_after(lines, index)).then_some(CodexBlockingScreen { title, detail })
}

fn is_structured_request_user_input(lines: &[String], selected_index: usize) -> bool {
    let start = selected_index.saturating_sub(20);
    let end = (selected_index + 12).min(lines.len());
    lines[start..end].iter().any(|line| {
        let line = strip_box_border(line).to_ascii_lowercase();
        line.starts_with("question ") && line.contains('/')
    })
}

/// Classify a live Codex screen that has replaced the ordinary composer with a
/// decision surface. `None` means either the composer is live, the selected UI
/// is stale scrollback, or the structured request_user_input path owns it.
pub fn detect_codex_blocking_screen(text: &str) -> Option<CodexBlockingScreen> {
    let lines = scan_lines(text);
    if lines.is_empty() {
        return None;
    }

    if let Some(screen) = live_named_screen(
        &lines,
        &["Chat stopped as a precaution"],
        "Codex stopped this chat as a precaution",
        "Codex will not accept another message in this chat. Choose New chat or Resume another chat in the terminal.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["This conversation is archived"],
        "Codex is waiting to unarchive this conversation",
        "Choose whether to unarchive the conversation or cancel before sending another message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Would you like to send input to terminal",
            "Would you like to send input to the existing terminal",
            "Would you like to run the following command",
            "Do you want to approve network access to",
            "Would you like to grant these permissions",
            "Would you like to make the following edits",
            "needs your approval.",
        ],
        "Codex is waiting for approval",
        "A command, edit, permission, terminal-input, network, or MCP action is waiting for your decision in the terminal.",
    ) {
        return Some(screen);
    }

    let flattened = lines.join(" ").to_ascii_lowercase();
    let mcp_field = lines.iter().rposition(|line| {
        let line = strip_box_border(line).to_ascii_lowercase();
        line.starts_with("field ") && line.contains('/')
    });
    if let Some(anchor) = mcp_field {
        if flattened.contains("esc to cancel")
            && (flattened.contains("to submit") || flattened.contains("to submit all"))
            && !composer_after(&lines, anchor)
        {
            return Some(CodexBlockingScreen {
                title: "Codex is waiting for an MCP response",
                detail: "An MCP server form is waiting for required fields or an approval decision in the terminal.",
            });
        }
    }

    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Install this app in your browser, then return here",
            "Enable this app to use it for the current request",
            "Sign in to this app in your browser, then return here",
            "Complete the requested action in your browser, then return here",
        ],
        "Codex is waiting for an app action",
        "Finish the requested app setup, sign-in, or external action, then choose how to continue in the terminal.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Welcome to Codex, OpenAI's command-line coding agent",
            "Use your own OpenAI API key for usage-based billing",
            "Signed in with your ChatGPT account",
        ],
        "Codex setup is waiting for input",
        "Finish or continue the Codex sign-in and first-run setup in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["Hooks need review"],
        "Codex is waiting for hook review",
        "Review, trust, or skip the changed hooks in the terminal before Codex starts the session.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Set up the Codex agent sandbox",
            "requires the default Codex agent sandbox to continue",
            "Couldn't set up your sandbox with Administrator permissions",
            "The Windows sandbox cannot guarantee protection",
        ],
        "Codex is waiting for Windows sandbox setup",
        "Choose a sandbox setup or safety option in the terminal before Codex can accept input.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Set up Amazon Bedrock",
            "Choose how you authenticate with AWS",
            "Choose an AWS profile",
        ],
        "Codex is waiting for Amazon Bedrock setup",
        "Choose the AWS authentication method or profile in the terminal before continuing.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Choose how you'd like Codex to proceed",
            "Bring over supported setup from another coding agent",
            "Choose what to import",
            "Choose an import source",
        ],
        "Codex is waiting for a migration choice",
        "Finish the model or external-agent configuration migration choice in the terminal before continuing.",
    ) {
        return Some(screen);
    }

    if let Some(index) = latest_line_containing(&lines, &["Press enter to continue"]) {
        if !composer_after(&lines, index) {
            return Some(CodexBlockingScreen {
                title: "Codex is waiting to continue",
                detail:
                    "A Codex setup or migration screen is waiting for confirmation in the terminal.",
            });
        }
    }

    if let Some(footer_index) = latest_modal_footer(&lines) {
        if !composer_after(&lines, footer_index)
            && !is_structured_request_user_input(&lines, footer_index)
        {
            return Some(CodexBlockingScreen {
                title: "Codex is waiting for input in a terminal dialog",
                detail: "A Codex picker or text prompt has replaced the chat composer. Complete or close it in the terminal before sending another message.",
            });
        }
    }

    let selected_index = latest_selected_choice(&lines)?;
    if composer_after(&lines, selected_index)
        || is_structured_request_user_input(&lines, selected_index)
    {
        return None;
    }
    Some(CodexBlockingScreen {
        title: "Codex is waiting for a choice",
        detail: "A Codex menu has replaced the input box. Make or cancel the selection in the terminal before sending another message.",
    })
}
