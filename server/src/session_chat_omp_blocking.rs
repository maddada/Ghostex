/*
CDXC:SessionChatOmpBlockingScreens 2026-08-31:
Oh My Pi audit against can1357/oh-my-pi 96906220. OMP has diverged from Pi's
focused-component grammar and deliberately owns a separate detector. Ordinary
prompt input is unavailable while OMP presents any of these terminal states:

  * tool approval, extension select/confirm/input/editor, and the rich Ask
    dialog (including custom answers and notes);
  * settings, advisor, model, thinking, theme, queue, image, history, session,
    tree, account, plugin, extension, agent, copy, move, and reset selectors;
  * plan review/save, session information, MCP configuration/authentication,
    OAuth provider/login/manual-code, and credential entry screens;
  * setup, missing-session-directory confirmation, pause, realtime voice, and
    the cancellable `/btw`, `/omfg`, `/cleanse`, and command-loader panels.

OMP's standard inline and fullscreen surfaces use `OverlayPanel`/overlay-box:
a rounded `╭ ... ╮` through `╰ ... ╯` frame with stable titles or action
footers. The normal composer is also rounded, so frames carrying OMP's `π`,
`⬢`, and `◒` composer header are explicitly excluded. A later live composer
header retires an earlier match, preventing a dismissed selector left in
scrollback from keeping chat blocked. Unframed states require paired, exact UI
evidence, never a generic word such as "Settings", "Ask", or "paused" alone.

Extensions may supply arbitrary `ui.custom` components or replace the editor
permanently. Standard custom components using OMP's overlay/control grammar are
covered; arbitrary components with no stable frame, title, or controls cannot
be identified safely from terminal text alone. Composer readiness still holds
sends whenever such a component hides the measured OMP composer.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const OMP_BLOCKING_SCAN_LINES: usize = 240;
const OMP_PAIR_RADIUS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OmpBlockingScreen {
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
        if lines.len() >= OMP_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn is_rounded_top(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('\u{256d}') && line.ends_with('\u{256e}')
}

fn is_rounded_bottom(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('\u{2570}') && line.ends_with('\u{256f}')
}

fn is_square_top(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('\u{250c}') && line.ends_with('\u{2510}')
}

fn is_square_bottom(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('\u{2514}') && line.ends_with('\u{2518}')
}

fn is_horizontal_rule(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= 8
        && line
            .chars()
            .all(|character| matches!(character, '\u{2500}' | '-' | '_'))
}

fn is_omp_composer_head(line: &str) -> bool {
    is_rounded_top(line)
        && line.contains('\u{03c0}')
        && line.contains('\u{2b22}')
        && line.contains('\u{25d2}')
}

fn latest_omp_composer_head(lines: &[String]) -> Option<usize> {
    lines.iter().rposition(|line| is_omp_composer_head(line))
}

fn composer_after(lines: &[String], evidence: usize) -> bool {
    latest_omp_composer_head(lines).is_some_and(|composer| composer > evidence)
}

fn rounded_frames(lines: &[String]) -> Vec<(usize, usize)> {
    let mut frames = Vec::new();
    for end in (0..lines.len()).rev() {
        if !is_rounded_bottom(&lines[end]) {
            continue;
        }
        let Some(start) = lines[..end].iter().rposition(|line| is_rounded_top(line)) else {
            continue;
        };
        frames.push((start, end));
    }
    frames
}

fn frame_title(line: &str) -> String {
    let title = line
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\u{256d}' | '\u{256e}' | '\u{2500}' | '\u{252c}' | '\u{2524}' | '\u{251c}'
            ) {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    collapse_spaces(&title).to_ascii_lowercase()
}

fn folded(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| collapse_spaces(line))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn has_choice_controls(text: &str) -> bool {
    let action = contains_any(
        text,
        &[
            "enter select",
            "enter to select",
            "enter confirm",
            "enter to confirm",
            "enter apply",
            "enter choose",
            "enter pick",
            "enter configure",
            "enter open",
            "enter use",
            "enter replace",
            "enter assign",
            "enter/f add",
            "space toggle",
            "space enable/disable",
            "click/enter/space to toggle",
            "enter / click",
            "\u{23ce} confirm",
            "\u{21b5} log out account",
        ],
    );
    let exit = contains_any(
        text,
        &[
            "esc cancel",
            "esc: cancel",
            "esc close",
            "esc: close",
            "esc back",
            "esc keep",
            "esc quit",
            "esc to go back",
            "esc to close",
        ],
    );
    action && exit
}

fn has_text_controls(text: &str) -> bool {
    contains_any(
        text,
        &[
            "enter submit esc cancel",
            "enter to submit",
            "enter save esc cancel",
            "enter to save esc to cancel",
            "ctrl+q/ctrl+enter submit esc cancel ctrl+g external editor",
            "enter or ctrl+q submit esc cancel ctrl+g external editor",
            "escape to cancel, enter to submit",
            "enter to continue, esc to cancel",
            "enter to continue, esc to go back",
            "press enter to skip or continue",
            "enter save and quit esc cancel",
        ],
    )
}

fn has_modal_controls(text: &str) -> bool {
    contains_any(
        text,
        &[
            "esc dismiss",
            "esc cancel /btw",
            "esc cancel /omfg",
            "esc cancel /cleanse",
            "esc to resume",
            "esc enter space resume",
            "esc end",
            "esc to close",
        ],
    )
}

fn is_known_overlay_title(title: &str) -> bool {
    matches!(
        title,
        "queue mode"
            | "theme"
            | "show images"
            | "plugins"
            | "thinking level"
            | "spend a saved rate-limit reset"
            | "history"
            | "branch from message"
            | "settings"
            | "extension control center"
            | "models"
            | "switch model"
            | "switch task model"
            | "agents"
            | "agent hub"
            | "copy to clipboard"
            | "move to directory"
            | "plan review"
            | "save and quit"
            | "session info"
            | "add mcp server"
            | "session tree"
    ) || title.starts_with("resume session")
        || (title.starts_with("import ") && title.ends_with(" session"))
        || title.starts_with("advisor configuration")
        || title.starts_with("agent hub ")
        || title.starts_with("select a ") && title.ends_with(" account for this session")
        || title.starts_with("select ") && title.ends_with(" account to log out")
        || title.starts_with("/btw ")
        || title.starts_with("/omfg ")
        || title == "/cleanse"
        || title.starts_with("/cleanse ")
}

fn is_auth_title(title: &str) -> bool {
    title.starts_with("login to ")
        || title == "select provider to login"
        || title == "select provider to logout"
        || title.starts_with("select a ") && title.ends_with(" account for this session")
        || title.starts_with("select ") && title.ends_with(" account to log out")
}

fn latest_line_containing(lines: &[String], needle: &str) -> Option<usize> {
    let needle = needle.to_ascii_lowercase();
    lines
        .iter()
        .rposition(|line| line.to_ascii_lowercase().contains(&needle))
}

fn latest_paired_evidence(
    lines: &[String],
    anchors: &[&str],
    companions: &[&str],
) -> Option<usize> {
    for anchor in (0..lines.len()).rev() {
        let line = lines[anchor].to_ascii_lowercase();
        if !anchors.iter().any(|needle| line.contains(needle)) {
            continue;
        }
        let end = (anchor + OMP_PAIR_RADIUS + 1).min(lines.len());
        if let Some(companion) = (anchor..end).rev().find(|index| {
            let candidate = lines[*index].to_ascii_lowercase();
            companions.iter().any(|needle| candidate.contains(needle))
        }) {
            return Some(anchor.max(companion));
        }
    }
    None
}

fn live_unframed(
    lines: &[String],
    evidence: Option<usize>,
    title: &'static str,
    detail: &'static str,
) -> Option<OmpBlockingScreen> {
    let evidence = evidence?;
    (!composer_after(lines, evidence)).then_some(OmpBlockingScreen { title, detail })
}

fn classify_rounded_frame(lines: &[String], start: usize, end: usize) -> Option<OmpBlockingScreen> {
    if is_omp_composer_head(&lines[start]) || composer_after(lines, end) {
        return None;
    }
    let title = frame_title(&lines[start]);
    let text = folded(&lines[start..=end]);

    if text.contains("allow tool:")
        && text.contains("approve")
        && text.contains("deny")
        && has_choice_controls(&text)
    {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for tool approval",
            detail:
                "Approve or deny the pending tool call in the terminal before sending a message.",
        });
    }

    if is_auth_title(&title) {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for authentication",
            detail: "Finish the provider or account choice, browser sign-in, authorization-code, or credential step in the terminal.",
        });
    }

    if title == "ask" || title.starts_with("ask (") {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for an answer",
            detail:
                "Answer or cancel OMP's question dialog in the terminal before sending a message.",
        });
    }

    if contains_any(
        &text,
        &[
            "oauth client secret",
            "api key or token",
            "authorization code",
            "paste your api key",
            "password",
        ],
    ) && (has_text_controls(&text)
        || title == "add mcp server"
        || title.starts_with("login to "))
    {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for a protected value",
            detail: "Enter or cancel the requested secret, token, API key, or authorization code in the terminal before sending a message.",
        });
    }

    if has_text_controls(&text) {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for text input",
            detail: "A text field or editor has replaced OMP's ordinary composer. Submit or cancel it in the terminal before sending a message.",
        });
    }

    if has_choice_controls(&text) {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for a choice",
            detail: "A selector or confirmation dialog owns terminal input. Make or cancel the choice before sending a message.",
        });
    }

    if has_modal_controls(&text) {
        return Some(OmpBlockingScreen {
            title: "OMP has an active terminal panel",
            detail: "Finish, cancel, resume, or dismiss the focused OMP panel in the terminal before sending a message.",
        });
    }

    if is_known_overlay_title(&title) {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for terminal interaction",
            detail: "Close or finish OMP's focused menu, picker, configuration screen, or modal before sending a message.",
        });
    }

    None
}

/// Classify a live OMP surface that owns terminal input in place of its normal
/// composer. `None` means either the composer has returned, only stale
/// scrollback matched, or the screen has no source-stable blocking evidence.
pub fn detect_omp_blocking_screen(text: &str) -> Option<OmpBlockingScreen> {
    let lines = scan_lines(text);
    if lines.is_empty() {
        return None;
    }

    if let Some(screen) = live_unframed(
        &lines,
        latest_paired_evidence(
            &lines,
            &["setup step "],
            &["enter confirm", "esc skip", "ctrl+c exit setup"],
        ),
        "OMP setup is waiting for input",
        "Complete, skip, or exit the current OMP setup step in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_unframed(
        &lines,
        latest_paired_evidence(
            &lines,
            &["p a u s e d", "paused for"],
            &[
                "esc to resume",
                "space resume",
                "agents hold at their next step",
            ],
        ),
        "OMP is paused",
        "Resume the paused OMP session in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_unframed(
        &lines,
        latest_paired_evidence(
            &lines,
            &["session's directory no longer exists ("],
            &["move (re-root) it into the current directory? [y/n]"],
        ),
        "OMP is waiting to move the session",
        "Accept or decline the missing-directory re-root prompt in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_unframed(
        &lines,
        latest_paired_evidence(
            &lines,
            &["select a provider:", "select a provider to logout:"],
            &["enter number ("],
        ),
        "OMP is waiting for an authentication choice",
        "Choose the provider or account in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_unframed(
        &lines,
        latest_line_containing(
            &lines,
            "paste the authorization code (or full redirect url):",
        ),
        "OMP is waiting for an authorization code",
        "Paste or cancel the provider authorization response in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    for (start, end) in rounded_frames(&lines) {
        if let Some(screen) = classify_rounded_frame(&lines, start, end) {
            return Some(screen);
        }
    }

    for end in (0..lines.len()).rev() {
        if !is_square_bottom(&lines[end]) || !lines[end].to_ascii_lowercase().contains("esc end") {
            continue;
        }
        let start = end.saturating_sub(6);
        if lines[start..end].iter().any(|line| is_square_top(line))
            && lines[start..=end]
                .iter()
                .any(|line| line.to_ascii_lowercase().contains("space mute"))
            && !composer_after(&lines, end)
        {
            return Some(OmpBlockingScreen {
                title: "OMP live mode owns terminal input",
                detail: "End the realtime voice session in the terminal before sending a normal text message.",
            });
        }
    }

    if let Some(cancel) = latest_line_containing(&lines, "esc cancel") {
        let start = cancel.saturating_sub(6);
        let end = (cancel + 7).min(lines.len());
        let rules = lines[start..end]
            .iter()
            .filter(|line| is_horizontal_rule(line))
            .count();
        if rules >= 2 && !composer_after(&lines, cancel) {
            return Some(OmpBlockingScreen {
                title: "OMP has a cancellable operation open",
                detail:
                    "Wait for or cancel the focused terminal operation before sending a message.",
            });
        }
    }

    None
}
