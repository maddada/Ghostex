/*
CDXC:AgentScreenDetection 2026-08-31:
Grok Build audit against xai-org/grok-build bc7f02ed. The pager has several
independent input owners which can coexist with, replace, or visually cover the
ordinary boxed `❯` composer:

  * permission, ask-user-question, MCP elicitation, plan approval, subagent
    cancellation, rewind, and jump cards;
  * command/session/model/theme/doc/settings/memory/usage/extension/agent
    pickers and confirmation modals, plus media and fullscreen viewers;
  * welcome, login, device/manual authentication, account consent, folder
    trust, new-worktree, and import screens;
  * queued-prompt, comment, shell-command, and memory-note text editors.

This detector uses source-stable live chrome: complete modal frames with the
pager's `[x]` close control, exact card titles paired with forward/directional
controls, or exact action footers paired with their owning surface. A later
ordinary composer retires unframed evidence left in scrollback. The welcome
start menu and framed popups are exceptions because Grok intentionally paints
them over a still-visible composer.

Third-party extension widgets may paint arbitrary text, and inline historical
prompt editing currently has no textual mode label. Those cannot be identified
safely from a terminal capture; matching generic prose would turn transcript
content into false blocking notices. The composer-readiness gate still holds a
send whenever such a component actually hides the measured composer.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const GROK_BLOCKING_SCAN_LINES: usize = 260;
const GROK_PAIR_RADIUS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrokBlockingScreen {
    pub title: &'static str,
    pub detail: &'static str,
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn scan_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines().rev() {
        let line = collapse_spaces(normalize_spaces(&strip_ansi_sgr(raw)).trim());
        if line.is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= GROK_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn strip_box_border(line: &str) -> &str {
    const BORDERS: &[char] = &[
        '\u{2502}', '\u{2503}', '\u{2506}', '\u{250a}', '\u{250c}', '\u{2510}', '\u{2514}',
        '\u{2518}', '\u{256d}', '\u{256e}', '\u{256f}', '\u{2570}', '|',
    ];
    line.trim()
        .trim_start_matches(BORDERS)
        .trim_end_matches(BORDERS)
        .trim()
}

fn is_numbered_choice(text: &str) -> bool {
    let text = text.trim_start();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && matches!(
            text.chars().nth(digits),
            Some('.' | ')' | ':' | ' ' | '\u{00a0}')
        )
}

fn is_normal_composer_line(line: &str) -> bool {
    let trimmed = line.trim();
    if !(trimmed.starts_with('\u{2502}') || trimmed.starts_with('|'))
        || !(trimmed.ends_with('\u{2502}') || trimmed.ends_with('|'))
    {
        return false;
    }
    let inner = strip_box_border(trimmed);
    let Some(after) = inner.strip_prefix('\u{276f}') else {
        return false;
    };
    !is_numbered_choice(after)
}

fn composer_after(lines: &[String], evidence: usize) -> bool {
    lines[evidence.saturating_add(1)..]
        .iter()
        .any(|line| is_normal_composer_line(line))
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    let text = text.to_ascii_lowercase();
    needles.iter().any(|needle| text.contains(needle))
}

fn latest_pair(lines: &[String], anchors: &[&str], companions: &[&str]) -> Option<usize> {
    for anchor in (0..lines.len()).rev() {
        if !contains_any(&lines[anchor], anchors) {
            continue;
        }
        let end = (anchor + GROK_PAIR_RADIUS + 1).min(lines.len());
        if let Some(companion) = (anchor..end)
            .rev()
            .find(|index| contains_any(&lines[*index], companions))
        {
            return Some(anchor.max(companion));
        }
    }
    None
}

fn keyed_menu_row(line: &str, label: &str, key: &str) -> bool {
    let line = line.to_ascii_lowercase();
    line.contains(label) && line.split_whitespace().last() == Some(key)
}

fn latest_keyed_menu_pair(
    lines: &[String],
    first_label: &str,
    first_key: &str,
    second_label: &str,
    second_key: &str,
) -> Option<usize> {
    for first in (0..lines.len()).rev() {
        if !keyed_menu_row(&lines[first], first_label, first_key) {
            continue;
        }
        let end = (first + 13).min(lines.len());
        if let Some(second) = (first.saturating_add(1)..end)
            .rev()
            .find(|index| keyed_menu_row(&lines[*index], second_label, second_key))
        {
            return Some(first.max(second));
        }
    }
    None
}

fn latest_consent_menu(lines: &[String]) -> Option<usize> {
    for accept in (0..lines.len()).rev() {
        let accept_line = lines[accept].to_ascii_lowercase();
        if accept_line.split_whitespace().last() != Some("a") {
            continue;
        }
        let end = (accept + 13).min(lines.len());
        if let Some(quit) = (accept.saturating_add(1)..end)
            .rev()
            .find(|index| keyed_menu_row(&lines[*index], "quit", "q"))
        {
            return Some(accept.max(quit));
        }
    }
    None
}

fn live_pair(
    lines: &[String],
    anchors: &[&str],
    companions: &[&str],
    title: &'static str,
    detail: &'static str,
) -> Option<GrokBlockingScreen> {
    let evidence = latest_pair(lines, anchors, companions)?;
    (!composer_after(lines, evidence)).then_some(GrokBlockingScreen { title, detail })
}

fn is_frame_top(line: &str) -> bool {
    let line = line.trim();
    matches!(line.chars().next(), Some('\u{250c}' | '\u{256d}'))
        && matches!(line.chars().last(), Some('\u{2510}' | '\u{256e}'))
}

fn is_frame_bottom(line: &str) -> bool {
    let line = line.trim();
    matches!(line.chars().next(), Some('\u{2514}' | '\u{2570}'))
        && matches!(line.chars().last(), Some('\u{2518}' | '\u{256f}'))
}

fn latest_closeable_frame(lines: &[String]) -> Option<usize> {
    for end in (0..lines.len()).rev() {
        if !is_frame_bottom(&lines[end]) {
            continue;
        }
        let start = end.saturating_sub(GROK_PAIR_RADIUS);
        if lines[start..end].iter().rev().any(|line| {
            is_frame_top(line)
                && (line.contains("[\u{2717}]") || line.contains("[x]") || line.contains("[X]"))
        }) {
            return Some(end);
        }
    }
    None
}

fn latest_shortcut_surface(lines: &[String]) -> Option<usize> {
    for index in (0..lines.len()).rev() {
        let line = lines[index].to_ascii_lowercase();
        let paired = (line.contains("enter:save") && line.contains("esc:cancel"))
            || (line.contains("enter:submit") && line.contains("esc:cancel"))
            || (line.contains("enter:confirm") && line.contains("esc:"))
            || (line.contains("enter select") && line.contains("esc close"))
            || (line.contains("enter import") && line.contains("esc cancel"))
            || (line.contains("esc:close")
                && (line.contains("space:pause")
                    || line.contains("space:play")
                    || line.contains("right:fwd")))
            || (line.contains("esc:quit") && line.contains("space:fire"));
        if paired {
            return Some(index);
        }
    }
    None
}

fn has_special_composer(lines: &[String], prefix: char, label: &str) -> Option<usize> {
    for label_index in (0..lines.len()).rev() {
        if !lines[label_index]
            .to_ascii_lowercase()
            .contains(&label.to_ascii_lowercase())
        {
            continue;
        }
        let start = label_index.saturating_sub(8);
        if lines[start..=label_index]
            .iter()
            .any(|line| strip_box_border(line).starts_with(prefix))
        {
            return Some(label_index);
        }
    }
    None
}

/// Classify a live Grok Build surface that owns terminal input instead of the
/// ordinary prompt. `None` means normal input has returned, only stale
/// scrollback matched, or no source-stable blocking chrome is visible.
pub fn detect_grok_blocking_screen(text: &str) -> Option<GrokBlockingScreen> {
    let lines = scan_lines(text);
    if lines.is_empty() {
        return None;
    }

    if latest_closeable_frame(&lines).is_some() {
        return Some(GrokBlockingScreen {
            title: "Grok Build has an active terminal modal",
            detail: "Finish or close the focused Grok Build modal or viewer in the terminal before sending a message.",
        });
    }

    // The welcome menu deliberately coexists with a boxed composer. Its two
    // directional actions identify it without relying on the Grok logo or a
    // generic Quit row.
    if latest_keyed_menu_pair(&lines, "new worktree", "ctrl+w", "resume session", "f3").is_some() {
        return Some(GrokBlockingScreen {
            title: "Grok Build is waiting at its start menu",
            detail:
                "Start or resume a Grok Build session in the terminal before sending a message.",
        });
    }

    if latest_keyed_menu_pair(&lines, "login with ", "l", "quit", "q").is_some() {
        return Some(GrokBlockingScreen {
            title: "Grok Build is waiting for sign-in",
            detail: "Choose the account action and finish signing in from the terminal before sending a message.",
        });
    }

    if let Some(evidence) = latest_consent_menu(&lines) {
        if !composer_after(&lines, evidence) {
            return Some(GrokBlockingScreen {
                title: "Grok Build is waiting for account consent",
                detail: "Read and accept the account notice, or quit, before sending a message.",
            });
        }
    }

    if let Some(screen) = live_pair(
        &lines,
        &["enlarge the window to read this notice", "window too small"],
        &["quit"],
        "Grok Build cannot show its account consent notice",
        "Enlarge the terminal to read and accept the account notice, or quit, before sending a message.",
    ) {
        return Some(screen);
    }

    // Manual-token authentication has its own boxed `❯` field, which is
    // not the ordinary message composer. The exact instruction and submit
    // control distinguish it from transcript prose.
    if latest_pair(
        &lines,
        &["a browser window will open for authentication."],
        &["paste your token here", "enter submit"],
    )
    .is_some()
    {
        return Some(GrokBlockingScreen {
            title: "Grok Build is waiting for authentication",
            detail:
                "Finish the browser or pasted-token authentication step before sending a message.",
        });
    }

    let paired_screens: &[(&[&str], &[&str], &str, &str)] = &[
        (
            &["do you trust the contents of this directory?"],
            &["yes, proceed", "no, quit"],
            "Grok Build is waiting for folder trust",
            "Accept or decline the workspace trust question in the terminal before sending a message.",
        ),
        (
            &["login with ", "switch account"],
            &["quit"],
            "Grok Build is waiting for sign-in",
            "Choose the account action and finish signing in from the terminal before sending a message.",
        ),
        (
            &[
                "a browser window will open for authentication.",
                "approve in your browser to finish signing in.",
            ],
            &[
                "waiting for login to complete...",
                "waiting for approval...",
                "enter submit",
            ],
            "Grok Build is waiting for authentication",
            "Finish the browser, device-code, or pasted-token authentication step before sending a message.",
        ),
        (
            &["new worktree"],
            &["name (optional):", "enter = create", "esc = cancel"],
            "Grok Build is waiting for a worktree name",
            "Create or cancel the new worktree dialog in the terminal before sending a message.",
        ),
        (
            &["subagents are still running. stop them?"],
            &["stop running", "continue to run"],
            "Grok Build is waiting on running subagents",
            "Choose whether Grok Build should stop or keep its subagents running before sending a message.",
        ),
        (
            &["rewind to which turn?", "jump to which turn?"],
            &["(no preview)", "rewind conversation to", "yes, and don't ask again"],
            "Grok Build is waiting for a turn choice",
            "Complete or dismiss the rewind or jump chooser in the terminal before sending a message.",
        ),
        (
            &["a turn is currently running."],
            &["cancel turn and rewind", "let it finish"],
            "Grok Build is waiting to rewind",
            "Choose whether to cancel the running turn before rewinding, or dismiss the prompt.",
        ),
        (
            &["mcp \u{201c}"],
            &[
                "requests your input",
                "wants to open a url",
                "accept / toggle",
                "decline",
            ],
            "Grok Build is waiting for MCP input",
            "Complete, accept, decline, or cancel the MCP request in the terminal before sending a message.",
        ),
        (
            &["tab:plan", "quit plan"],
            &["approve", "request changes", "tab:prompt"],
            "Grok Build is waiting for plan approval",
            "Approve the plan or send revision feedback from the terminal before sending a normal message.",
        ),
        (
            &["ctrl+o:always-approve", "always-approve"],
            &["ctrl+c:cancel", "next option", "edit pattern"],
            "Grok Build is waiting for permission",
            "Approve, reject, edit, or cancel the pending tool permission in the terminal before sending a message.",
        ),
        (
            &["\u{2191}/\u{2193} navigate"],
            &["enter:submit", "enter:select", "tab:next answer", "x:dismiss"],
            "Grok Build is waiting for an answer",
            "Answer or dismiss Grok Build's question card in the terminal before sending a message.",
        ),
    ];
    for (anchors, companions, title, detail) in paired_screens {
        if let Some(screen) = live_pair(&lines, anchors, companions, title, detail) {
            return Some(screen);
        }
    }

    if let Some(evidence) = latest_shortcut_surface(&lines) {
        if !composer_after(&lines, evidence) {
            return Some(GrokBlockingScreen {
                title: "Grok Build is waiting for terminal interaction",
                detail: "Submit, confirm, cancel, or dismiss the focused Grok Build editor, picker, or fullscreen surface before sending a message.",
            });
        }
    }

    for (prefix, label, title, detail) in [
        (
            '!',
            "run shell command",
            "Grok Build is in shell-command mode",
            "Exit or finish the shell-command editor in the terminal before sending a normal message.",
        ),
        (
            '#',
            "save memory note",
            "Grok Build is editing a memory note",
            "Save or cancel the memory-note editor in the terminal before sending a normal message.",
        ),
    ] {
        if let Some(evidence) = has_special_composer(&lines, prefix, label) {
            if !composer_after(&lines, evidence) {
                return Some(GrokBlockingScreen { title, detail });
            }
        }
    }

    live_pair(
        &lines,
        &["editing queued #", "enter:save comment"],
        &["enter:save", "esc:cancel"],
        "Grok Build is editing text outside the composer",
        "Save or cancel the queued prompt or comment editor in the terminal before sending a normal message.",
    )
}
