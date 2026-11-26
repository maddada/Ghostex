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
dismissed selector left in scrollback from keeping chat blocked. The
exceptions are a tool approval and an Ask sitting directly on the composer (see
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
use crate::session_chat_resume_prompt::{
    SessionChatTerminalPicker, SessionChatTerminalPickerKind, SessionChatTerminalPickerRow,
};

const OMP_BLOCKING_SCAN_LINES: usize = 240;
const OMP_PAIR_RADIUS: usize = 100;
/// The title of the card for OMP's live tool approval when the chat does not answer it itself.
pub const OMP_TOOL_APPROVAL_TITLE: &str = "OMP is waiting for tool approval";

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

/// The screen's non-blank lines, trimmed and space-collapsed with key glyphs written as words; the index among them of OMP's live composer; the index of the line drawn directly on top of the composer, with no row between them (see `omp_composer_head`); and their original row indices.
fn scan_lines(text: &str) -> (Vec<String>, Option<usize>, Option<usize>, Vec<usize>) {
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
    let (composer, on_composer) = omp_composer_head(text, &rows);
    (lines, composer, on_composer, rows)
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
/// Most composer shapes are told apart by indentation (`❯ ` against two-space rows, the band's three-space rows), which the trimmed scan lines lose, so the composer is found on the untrimmed screen and its row mapped onto them.
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// The composer's line is the first one it draws: its top chrome (the box's `╭` border, the band's status row, a top rule), or its input's first row for the borderless, field and rail shapes. Mapping every shape to the last line above its input put those three on the foot of a selector dismissed above them, so the selector was never retired and their sends stayed held. A frame whose last line sits on the composer's first row with no row between them is mounted with it in OMP's editor slot.
fn omp_composer_head(text: &str, rows: &[usize]) -> (Option<usize>, Option<usize>) {
    let lines: Vec<_> = text.lines().map(strip_ansi_sgr).collect();
    let Some(top) = crate::session_chat_composer::omp_composer_top_row(&lines) else {
        return (None, None);
    };
    let head = rows.iter().filter(|&&row| row < top).count();
    let on_composer = head.checked_sub(1).filter(|&above| rows[above] + 1 == top);
    (Some(head), on_composer)
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
            // CDXC:AgentScreenDetection 2026-11-03 WHY: a tool approval with a draft typed into it (OMP 18.13.3's `approval-dialog.ts`: the first typed key goes to the composer below, the highlight moves to "Deny and send message") swaps its footer for "⏎ deny and send · … · ⎋ deny". Without this pair the approval stopped reading as one after the first key a chat card passed through, and the card went away while OMP still waited.
            "enter deny and send",
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
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// OMP 18.13.3 mounts an Ask that arrives while the user has a draft in the editor slot with the draft's editor right below it (`extension-ui-controller.ts`), and the arrow keys hand input to the Ask while the draft stays drawn as a readable composer. Retiring the Ask because that composer follows it read a pending question as a ready input once rows under the draft no longer hid it. An Ask or approval frame whose last row sits directly on the composer's first row is live; with any row between them, blank ones included, it stays retired.
fn is_live_approval(
    text: &str,
    composer: Option<usize>,
    on_composer: Option<usize>,
    end: usize,
) -> bool {
    text.contains("allow tool:")
        && text.contains("approve")
        && text.contains("deny")
        && has_choice_controls(text)
        && (on_composer == Some(end) || !composer_after(composer, end))
}

fn classify_rounded_frame(
    lines: &[String],
    composer: Option<usize>,
    on_composer: Option<usize>,
    start: usize,
    end: usize,
) -> Option<OmpBlockingScreen> {
    if composer == Some(start) {
        return None;
    }
    let title = frame_title(&lines[start]);
    let text = folded(&lines[start..=end]);

    let composer_on_frame = on_composer == Some(end);
    if is_live_approval(&text, composer, on_composer, end) {
        return Some(OmpBlockingScreen {
            title: OMP_TOOL_APPROVAL_TITLE,
            detail:
                "Approve or deny the pending tool call in the terminal before sending a message.",
        });
    }

    if (title == "ask" || title.starts_with("ask ("))
        && (composer_on_frame || !composer_after(composer, end))
    {
        return Some(OmpBlockingScreen {
            title: "OMP is waiting for an answer",
            detail:
                "Answer or cancel OMP's question dialog in the terminal before sending a message.",
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

/// CDXC:AgentScreenDetection 2026-11-03 WHY:
/// OMP's tool approval is answered in the chat from its own rows, the way Claude Code's, Antigravity's, Copilot's and Grok's permission prompts are: the dialog's choices (the grants and their remember toggle when offered, Approve, Approve for this session, Deny) in the order OMP lists them, with OMP's highlight. Its rows carry no numbers, so a pick walks the highlight and presses Enter. Once a key typed in OMP's terminal turned the dialog into "Deny and send message" (the footer says "deny and send"), Enter sends that message, so the dialog is left to the pass-through card the blocking detector above makes of it.
pub fn detect_omp_permission_picker(text: &str) -> Option<SessionChatTerminalPicker> {
    read_omp_permission_picker(text).map(|(picker, _)| picker)
}

/// The same live picker with the command lines its dialog actually shows, for the folded card.
pub fn detect_omp_permission_picker_with_command(
    text: &str,
) -> Option<(SessionChatTerminalPicker, Option<String>)> {
    let (picker, request) = read_omp_permission_picker(text)?;
    let mut command = String::new();
    let mut found = false;
    for raw in text.lines().skip(request.start).take(request.len()) {
        let raw = strip_ansi_sgr(raw);
        let line = raw.trim();
        let Some(line) = line
            .strip_prefix('\u{2502}')
            .or_else(|| line.strip_prefix('\u{2503}'))
            .and_then(|line| {
                line.strip_suffix('\u{2502}')
                    .or_else(|| line.strip_suffix('\u{2503}'))
            })
        else {
            continue;
        };
        let line = line.strip_prefix(' ').unwrap_or(line).trim_end();
        if found {
            // OMP appends the grant's folder and provider safety checks after tool details.
            // Expanded commands keep their own newlines, without continuation indentation.
            if line.starts_with("Folder:") || line == "Provider safety checks:" {
                break;
            }
            command.push('\n');
            command.push_str(line);
        } else if let Some(value) = line.strip_prefix("Command:") {
            command.push_str(value.strip_prefix(' ').unwrap_or(value));
            found = true;
        }
    }
    Some((picker, (!command.trim().is_empty()).then_some(command)))
}

/// How OMP's live approval dialog relates to the request a chat card names by its summary
/// (`summarize_approval_input`: the command, path, URL or pattern, else the input's JSON, cut at
/// 200 characters with an ellipsis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OmpRequestMatch {
    /// The dialog shows exactly the card's request.
    Same,
    /// The dialog shows another request.
    Other,
    /// What the dialog or the card shows could stand for more than one request.
    NotUnique,
}

/// The one request an OMP approval dialog shows, read back from the lines OMP 18.21 draws for it:
/// an MCP tool's `Arguments:` followed by the whole arguments as JSON.
#[derive(Clone, Debug, PartialEq)]
pub enum OmpShownRequest {
    Arguments(serde_json::Value),
}

/*
CDXC:SessionChat 2026-11-25 DECISION:
User: a stale card never answers another request; an approval Ghostex cannot tie to its request
is answered in the terminal, with neither Allow nor Deny from the card; two requests whose text
is exactly identical may answer each other.

CDXC:SessionChat 2026-11-26 DECISION:
User: bash approvals in OMP are answered in the terminal too, on the phone and in a coordinator,
because the screen cannot show whether a command ends in spaces.

CDXC:SessionChat 2026-11-25 WHY:
So a dialog's request is compared as its content, and only OMP's own display formatting is
removed: the frame, its padding and the indentation of each line (OMP 18.21 draws an MCP tool's
arguments as indented JSON under `Arguments:`, whose indentation is JSON whitespace). Wherever the
drawing cannot be read back without loss, the dialog shows no unique request: bash's
`Command: ${command}` (one line in a frame padded with spaces, so trailing whitespace never
shows), anything OMP or the card cut (an ellipsis, OMP's "… elided", "… allowed segments"), text
with control or invisible characters, arguments that are no whole JSON, arguments the card names
only by their path, and any other tool's own approval details. The 2026-11-07 check passed a card
whose summary occurred anywhere in the dialog, and the first 2026-11-25 version compared both
sides with their whitespace collapsed, so `printf 'a  b'` passed for `printf 'a b'`.
*/
/// The request `request` (a dialog's lines below its title) shows, when it shows exactly one.
pub fn omp_shown_request(request: &[String]) -> Option<OmpShownRequest> {
    let mut markers = request
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("Command:") || line.starts_with("Arguments:"));
    let (start, _) = markers.next()?;
    if markers.next().is_some() {
        return None;
    }
    // OMP appends the grant's folder and provider safety checks after the tool's details.
    let end = request[start..]
        .iter()
        .position(|line| line.starts_with("Folder:") || line == "Provider safety checks:")
        .map_or(request.len(), |offset| start + offset);
    let shown = &request[start..end];
    if shown.iter().any(|line| line.contains('\u{2026}')) {
        return None;
    }
    // bash's `Command: <command>` cannot be read back without loss: OMP draws it on one line in a
    // frame padded with spaces, so a command's trailing whitespace never shows, and two commands
    // that differ only there look the same. So it shows no unique request, and its approval is
    // answered in the terminal (CDXC:SessionChat 2026-11-26 DECISION above).
    if shown[0].starts_with("Command:") {
        return None;
    }
    let arguments = shown
        .join("\n")
        .strip_prefix("Arguments:")
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())?;
    json_reads_back_verbatim(&arguments).then_some(OmpShownRequest::Arguments(arguments))
}

/// Text a terminal frame shows as it is: no control characters (tabs, line breaks, escapes) and
/// no invisible ones (zero-width, bidirectional marks), which a terminal draws differently or not
/// at all.
fn reads_back_verbatim(text: &str) -> bool {
    !text.chars().any(|c| {
        c.is_control()
            || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}')
    })
}

fn json_reads_back_verbatim(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => reads_back_verbatim(text),
        serde_json::Value::Array(items) => items.iter().all(json_reads_back_verbatim),
        serde_json::Value::Object(map) => map
            .iter()
            .all(|(key, value)| reads_back_verbatim(key) && json_reads_back_verbatim(value)),
        _ => true,
    }
}

/// Whether the dialog shows exactly the card's request (CDXC:SessionChat 2026-11-25 DECISION
/// above `omp_shown_request`).
pub fn omp_permission_request_match(
    dialog: &OmpPermissionDialog,
    summary: &str,
) -> OmpRequestMatch {
    let Some(shown) = dialog.shown.as_ref() else {
        return OmpRequestMatch::NotUnique;
    };
    if summary.contains('\u{2026}') {
        return OmpRequestMatch::NotUnique;
    }
    match shown {
        OmpShownRequest::Arguments(arguments) => {
            if arguments.to_string() == summary {
                OmpRequestMatch::Same
            } else if crate::session_chat_interactive::summarize_approval_input(Some(arguments))
                == summary
            {
                OmpRequestMatch::NotUnique
            } else {
                OmpRequestMatch::Other
            }
        }
    }
}

/// The keys that answer a chat card for OMP's approval of `tool` with `summary` on `text`, read
/// from that same screen: the Approve row's for Allow, Escape for Deny. `None` unless the dialog
/// is that tool's and shows exactly the card's request (`omp_permission_request_match`), so
/// nothing is typed into another request's dialog.
pub fn omp_card_answer_key(text: &str, tool: &str, summary: &str, allow: bool) -> Option<String> {
    let (picker, _, dialog) = read_omp_approval_frame(text)?;
    let normalize = crate::session_chat_interactive::normalize_session_chat_tool_name;
    if normalize(&dialog.tool) != normalize(tool)
        || omp_permission_request_match(&dialog, summary) != OmpRequestMatch::Same
    {
        return None;
    }
    if !allow {
        return Some(crate::session_chat_send::SESSION_CHAT_INTERRUPT.to_string());
    }
    let approve = picker
        .rows
        .iter()
        .position(|row| matches!(row.label.as_str(), "Approve" | "Approve Once"))?;
    picker.answer_key(approve)
}

/// What a pick of one of OMP's own rows on the chat's screen card was made on: the dialog's tool,
/// its request lines, its rows and the highlighted row, as the answer's admission read them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmpRowPick {
    pub tool: String,
    pub request: Vec<String>,
    pub rows: Vec<String>,
    pub selected: usize,
    pub choice_index: usize,
}

/// The pick of row `choice_index` on the dialog `text` shows, when that dialog shows a unique
/// request (`omp_shown_request`) and has that row. `None` otherwise: a dialog without a unique
/// request has no rows on the chat's card.
pub fn omp_row_pick(text: &str, choice_index: usize) -> Option<OmpRowPick> {
    read_omp_row_pick(text, choice_index).map(|(_, pick)| pick)
}

/// The keys for `pick` on `text`, read from that same screen: `None` unless the dialog is still
/// the one the pick was made on (its tool, its whole request, its rows and its highlight), so a
/// row picked for one request is never typed into another's dialog.
pub fn omp_row_answer_key(text: &str, pick: &OmpRowPick) -> Option<String> {
    let (picker, now) = read_omp_row_pick(text, pick.choice_index)?;
    if now != *pick {
        return None;
    }
    picker.answer_key(pick.choice_index)
}

fn read_omp_row_pick(
    text: &str,
    choice_index: usize,
) -> Option<(SessionChatTerminalPicker, OmpRowPick)> {
    let (picker, _, dialog) = read_omp_approval_frame(text)?;
    dialog.shown.as_ref()?;
    picker.rows.get(choice_index)?;
    let pick = OmpRowPick {
        tool: dialog.tool,
        request: dialog.request,
        rows: picker.rows.iter().map(|row| row.label.clone()).collect(),
        selected: picker.selected_index,
        choice_index,
    };
    Some((picker, pick))
}

/// What OMP's live approval dialog asks about: the tool its `Allow tool: <name>` title names, the
/// request lines below it (OMP 18.21 draws an MCP tool's origin, the reason it asks, then a tool's
/// `formatApprovalDetails` or an MCP tool's `Arguments:`), and the one request those lines show,
/// when they show one (`omp_shown_request`).
#[derive(Clone, Debug, PartialEq)]
pub struct OmpPermissionDialog {
    pub tool: String,
    pub request: Vec<String>,
    pub shown: Option<OmpShownRequest>,
}

/// The live approval dialog, `None` when none is up (or it reads as "deny and send").
pub fn read_omp_permission_dialog(text: &str) -> Option<OmpPermissionDialog> {
    read_omp_approval_frame(text).map(|(_, _, dialog)| dialog)
}

/// CDXC:SessionChat 2026-11-25 WHY:
/// The owner's decision above `omp_shown_request` covers OMP's eval-cell approvals too. A dialog
/// that shows no unique request (a tool-only dialog such as "Allow tool: commit" with nothing below
/// it, a cut or segmented command, another tool's own details) can look the same for two calls,
/// and a row picked on one would answer the next, so it is no picker: the blocking card above
/// (`OMP_TOOL_APPROVAL_TITLE`, Open terminal) is its card.
/// SEE-ALSO: server/src/session_chat_send/answer_http.rs (`verify_omp_hook_answer_targets_dialog`).
fn read_omp_permission_picker(
    text: &str,
) -> Option<(SessionChatTerminalPicker, std::ops::Range<usize>)> {
    read_omp_approval_frame(text)
        .filter(|(_, _, dialog)| dialog.shown.is_some())
        .map(|(picker, request, _)| (picker, request))
}

fn read_omp_approval_frame(
    text: &str,
) -> Option<(
    SessionChatTerminalPicker,
    std::ops::Range<usize>,
    OmpPermissionDialog,
)> {
    let (lines, composer, on_composer, raw_rows) = scan_lines(text);
    let (start, end) = rounded_frames(&lines).into_iter().find(|&(start, end)| {
        composer != Some(start)
            && is_live_approval(&folded(&lines[start..=end]), composer, on_composer, end)
    })?;
    if folded(&lines[start..=end]).contains("enter deny and send") {
        return None;
    }
    let inner = |line: &str| {
        line.trim()
            .trim_matches(|c| matches!(c, '\u{2502}' | '\u{2503}'))
            .trim()
            .to_string()
    };
    let divider =
        (start + 1..end).rfind(|index| lines[*index].trim_start().starts_with('\u{251c}'))?;
    let mut rows = Vec::new();
    for line in lines[start + 1..divider].iter().rev() {
        let line = inner(line);
        let (selected, label) = match ["\u{276f} ", "\u{f054} ", "> "]
            .iter()
            .find_map(|cursor| line.strip_prefix(*cursor))
        {
            Some(label) => (true, label.trim()),
            None => (false, line.as_str()),
        };
        if !(label == "Deny"
            || label.starts_with("Approve")
            || label.starts_with("Always Approve")
            || label.ends_with("Remember across sessions"))
        {
            break;
        }
        rows.push(SessionChatTerminalPickerRow {
            number: 0,
            label: label.to_string(),
            selected,
        });
    }
    rows.reverse();
    let selected_index = rows.iter().position(|row| row.selected)?;
    let request = raw_rows[start + 1]..raw_rows[divider - rows.len()];
    let detail = std::iter::once(&lines[start])
        .chain(&lines[start + 1..divider - rows.len()])
        .map(|line| {
            inner(line)
                .trim_matches(|c| matches!(c, '\u{256d}' | '\u{256e}' | '\u{2500}'))
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let title = inner(&lines[start])
        .trim_matches(|c| matches!(c, '\u{256d}' | '\u{256e}' | '\u{2500}'))
        .trim()
        .to_string();
    let request_lines: Vec<String> = text
        .lines()
        .skip(request.start)
        .take(request.len())
        .map(|raw| {
            strip_ansi_sgr(raw)
                .trim()
                .trim_matches(|c| matches!(c, '\u{2502}' | '\u{2503}'))
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect();
    let shown = omp_shown_request(&request_lines);
    Some((
        SessionChatTerminalPicker {
            kind: SessionChatTerminalPickerKind::OmpPermission,
            detail: (!detail.is_empty()).then_some(detail),
            rows,
            selected_index,
        },
        request,
        OmpPermissionDialog {
            tool: title
                .strip_prefix("Allow tool:")
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
            request: request_lines,
            shown,
        },
    ))
}

/// Classify a live OMP surface that owns terminal input in place of its normal
/// composer. `None` means either the composer has returned, only stale
/// scrollback matched, or the screen has no source-stable blocking evidence.
pub fn detect_omp_blocking_screen(text: &str) -> Option<OmpBlockingScreen> {
    let (lines, composer, on_composer, _) = scan_lines(text);
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
        if let Some(screen) = classify_rounded_frame(&lines, composer, on_composer, start, end) {
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
