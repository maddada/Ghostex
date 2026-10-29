/*
CDXC:AgentScreenDetection 2026-08-31:
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
footers. The normal composer is also rounded, so the live composer's own frame
is excluded, and a live composer below a match retires it, preventing a
dismissed selector left in scrollback from keeping chat blocked. The one
exception is a tool approval sitting directly on the composer (see
`classify_rounded_frame`). Unframed states require paired, exact UI
evidence, never a generic word such as "Settings", "Ask", or "paused" alone.

Extensions may supply arbitrary `ui.custom` components or replace the editor
permanently. Standard custom components using OMP's overlay/control grammar are
covered; arbitrary components with no stable frame, title, or controls cannot
be identified safely from terminal text alone. Composer readiness still holds
sends whenever such a component hides the measured OMP composer.
*/

use std::borrow::Cow;

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const OMP_BLOCKING_SCAN_LINES: usize = 240;
const OMP_PAIR_RADIUS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OmpBlockingScreen {
    pub title: &'static str,
    pub detail: &'static str,
}

const LIVE_MODE: OmpBlockingScreen = OmpBlockingScreen {
    title: "OMP live mode owns terminal input",
    detail: "End the realtime voice session in the terminal before sending a normal text message.",
};

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// OMP's key glyphs in its Unicode and Nerd Font symbol presets, with the words its ASCII preset writes for them. A Nerd Font modifier is followed by a space before its key.
const OMP_KEY_GLYPH_WORDS: &[(&str, &str)] = &[
    ("\u{2303}", "ctrl+"),
    ("\u{f0634} ", "ctrl+"),
    ("\u{23ce}", "enter"),
    ("\u{f0311}", "enter"),
    ("\u{238b}", "esc"),
    ("\u{f12b7}", "esc"),
    ("\u{2423}", "space"),
    ("\u{f1050}", "space"),
];

/// CDXC:AgentScreenDetection 2026-10-28 WHY:
/// Since 18.3.3 OMP draws every key hint with its symbol preset's key labels: `⏎ select · ⎋ cancel` with Unicode symbols, Nerd Font icons, and `Enter select · Esc cancel` only with ASCII symbols. The needles below are written in those words, so with the default Unicode preset no hint matched and a tool approval, the setup wizard or a pause screen read as no blocking screen. Writing the glyphs as words keeps one needle per hint for every preset.
fn omp_key_glyphs_as_words(line: &str) -> Cow<'_, str> {
    let has_glyph = |c: char| {
        OMP_KEY_GLYPH_WORDS
            .iter()
            .any(|(glyph, _)| glyph.starts_with(c))
    };
    if !line.contains(has_glyph) {
        return Cow::Borrowed(line);
    }
    Cow::Owned(
        OMP_KEY_GLYPH_WORDS
            .iter()
            .fold(line.to_string(), |line, (glyph, word)| {
                line.replace(glyph, word)
            }),
    )
}

/// The screen's non-blank lines, trimmed and space-collapsed with key glyphs written as words, and the index among them of the last line above OMP's live composer (see `omp_composer_head`).
fn scan_lines(text: &str) -> (Vec<String>, Option<usize>) {
    let mut lines = Vec::new();
    let mut rows = Vec::new();
    let raw_lines: Vec<_> = text.lines().collect();
    for (row, raw) in raw_lines.iter().enumerate().rev() {
        let line = collapse_spaces(&omp_key_glyphs_as_words(
            normalize_spaces(&strip_ansi_sgr(raw)).trim(),
        ));
        if line.is_empty() {
            continue;
        }
        lines.push(line);
        rows.push(row);
        if lines.len() >= OMP_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    rows.reverse();
    (lines, omp_composer_head(text, &rows))
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

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// The composer used to be recognised by the `π`, `⬢` and `◒` in its statusline border, which OMP's Nerd Font symbol preset never draws, so a dismissed dialog kept blocking sends. It is found by its frame instead, the same shape composer readiness reads (`omp_input_region`).
/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// Most composer shapes are told apart by indentation (`❯ ` against two-space rows, the band's three-space rows), which the trimmed scan lines lose, so the composer is found on the untrimmed screen and its row mapped onto them: the last scan line above the input, which for the box is its `╭` border.
fn omp_composer_head(text: &str, rows: &[usize]) -> Option<usize> {
    let lines: Vec<_> = text.lines().map(strip_ansi_sgr).collect();
    let input = crate::session_chat_composer::omp_composer_input_row(&lines)?;
    rows.iter()
        .filter(|&&row| row < input)
        .count()
        .checked_sub(1)
}

fn composer_after(composer: Option<usize>, evidence: usize) -> bool {
    composer.is_some_and(|composer| composer > evidence)
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
            "\u{21b5} log out account",
            "enter log out account",
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
            "esc deny",
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
            "enter save · esc cancel",
            "enter to save esc to cancel",
            "enter to save · esc to cancel",
            "ctrl+q/ctrl+enter submit esc cancel ctrl+g external editor",
            "enter or ctrl+q submit esc cancel ctrl+g external editor",
            "escape to cancel, enter to submit",
            "esc to cancel, enter to submit",
            "enter to continue, esc to cancel",
            "enter to continue, esc to go back",
            "press enter to skip or continue",
            "enter save and quit esc cancel",
            "enter save and quit · esc cancel",
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
            "esc · enter · space — resume",
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
            | "rewind"
            | "settings"
            | "extension control center"
            | "models"
            | "switch model"
            | "switch task model"
            | "select model"
            | "agents"
            | "agent hub"
            | "copy to clipboard"
            | "copy"
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
    composer: Option<usize>,
    evidence: Option<usize>,
    title: &'static str,
    detail: &'static str,
) -> Option<OmpBlockingScreen> {
    let evidence = evidence?;
    (!composer_after(composer, evidence)).then_some(OmpBlockingScreen { title, detail })
}

/// CDXC:AgentScreenDetection 2026-10-28 WHY:
/// OMP 18.13 draws a tool approval inline and keeps its composer on screen right below it (the `pi` and the `box` shape alike), and removes the frame as soon as the call is approved or denied. The stale-scrollback rule, which retires a frame with the live composer below it, therefore dropped every live approval: the chat showed no card, the composer read as ready, and a chat message would have reached OMP, whose footer reads "type to deny and steer". An approval frame is live while nothing but the composer follows it; with any line between them it stays retired like every other frame.
fn classify_rounded_frame(
    lines: &[String],
    composer: Option<usize>,
    start: usize,
    end: usize,
) -> Option<OmpBlockingScreen> {
    if composer == Some(start) {
        return None;
    }
    let title = frame_title(&lines[start]);
    let text = folded(&lines[start..=end]);

    if text.contains("allow tool:")
        && text.contains("approve")
        && text.contains("deny")
        && has_choice_controls(&text)
        && (composer == Some(end + 1) || !composer_after(composer, end))
    {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for tool approval",
            detail:
                "Approve or deny the pending tool call in the terminal before sending a message.",
        });
    }

    if composer_after(composer, end) {
        return None;
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
            "client secret",
            "api key or token",
            "authorization code",
            "paste your api key",
            "paste the key or token",
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

    // OMP 18.13 draws live mode in a rounded frame; older versions used the square one handled in `detect_omp_blocking_screen`.
    if text.contains("space mute") && text.contains("esc end") {
        return Some(LIVE_MODE);
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
    let (lines, composer) = scan_lines(text);
    if lines.is_empty() {
        return None;
    }

    if let Some(screen) = live_unframed(
        composer,
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
        composer,
        latest_paired_evidence(
            &lines,
            &["p a u s e d", "paused for"],
            &[
                "esc to resume",
                "space resume",
                "space — resume",
                "hold at their next step",
            ],
        ),
        "OMP is paused",
        "Resume the paused OMP session in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_unframed(
        composer,
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
        composer,
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
        composer,
        latest_line_containing(
            &lines,
            "paste the authorization code (or full redirect url)",
        ),
        "OMP is waiting for an authorization code",
        "Paste or cancel the provider authorization response in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    for (start, end) in rounded_frames(&lines) {
        if let Some(screen) = classify_rounded_frame(&lines, composer, start, end) {
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
            && !composer_after(composer, end)
        {
            return Some(LIVE_MODE);
        }
    }

    if let Some(cancel) = latest_line_containing(&lines, "esc cancel") {
        let start = cancel.saturating_sub(6);
        let end = (cancel + 7).min(lines.len());
        let rules = lines[start..end]
            .iter()
            .filter(|line| is_horizontal_rule(line))
            .count();
        if rules >= 2 && !composer_after(composer, cancel) {
            return Some(OmpBlockingScreen {
                title: "OMP has a cancellable operation open",
                detail:
                    "Wait for or cancel the focused terminal operation before sending a message.",
            });
        }
    }

    None
}
