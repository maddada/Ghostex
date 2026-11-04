use std::ops::Range;

use super::{is_horizontal_rule, is_titled_horizontal_rule, strip_ansi_sgr};

/// CDXC:SessionChat 2026-09-15 DECISION:
/// User: support sending ! shell commands from Chat for Claude agents, like Codex.
/// Claude replaces its normal prompt glyph with !, so readiness and draft readers must accept both inside the input frame.
pub(super) const CLAUDE_COMPOSER_MARKERS: &[char] = &['❯', '!'];

/// CDXC:SessionChat 2026-09-08 WHY:
/// Rewind restores wrapped drafts in both Claude and Codex. Claude's former three-row signature rejected those drafts before Send could clear them.
/// Readiness, rewind and replacement must agree on the whole input region, including empty and continued rows.
pub(super) fn rule_input_region(lines: &[String], markers: &[char]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_titled_horizontal_rule(line))?;
    let start = (head + 1..foot).find(|&i| !lines[i].trim().is_empty())?;
    lines[start]
        .trim_start()
        .starts_with(markers)
        .then_some(start..foot)
}

/// CDXC:AgentScreenDetection 2026-10-11 WHY:
/// While any background task runs, Antigravity CLI (measured on 1.3.3) draws its active-items list between rules under the input box, one `● [09:47:11] ping -n 900 127.0.0.1 running` row per task. The last rule pair then framed the task rows, so the input box read as missing: every chat send was refused ("The antigravity input box is not on screen yet"), and a model change, and every message held behind it or behind a waking session, waited forever while the terminal took input normally (a 2026-10-11 customer report, reproduced with a dev-server task). The input box is the sandwich above that list; any other block under it still reads as no input box.
pub(super) fn antigravity_input_region(lines: &[String]) -> Option<Range<usize>> {
    if let Some(region) = rule_input_region(lines, &['>']) {
        return Some(region);
    }
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_horizontal_rule(line))?;
    let mut rows = lines[head + 1..foot]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .peekable();
    if rows.peek().is_none() || !rows.all(|line| is_antigravity_active_item(line)) {
        return None;
    }
    rule_input_region(&lines[..=head], &['>'])
}

/// The hints Antigravity CLI 1.4.9 shows in its empty input row after Shift+Tab leaves Default
/// mode (Default shows none). Read from the shipped binary and checked live on 2026-10-29.
const ANTIGRAVITY_MODE_HINTS: &[&str] = &[
    "Accept-edits mode: file edits auto-approved (shift+tab to cycle)",
    "Auto mode: edits & safe commands auto-approved (shift+tab to cycle)",
    "Plan mode: research & plan only (shift+tab to cycle)",
    "Best-of-N mode: fork into parallel arms and pick a winner (shift+tab to cycle)",
];

/// CDXC:AgentScreenDetection 2026-10-29 WHY:
/// Antigravity CLI 1.4.7 and 1.4.9 show a mode hint where typed text goes (`> Accept-edits mode: file edits auto-approved (shift+tab to cycle)`, cut to `(shif...` on a narrow pane), drawn in a muted colour rather than SGR faint (256-colour 109, which zmx's capture serializes as RGB 154,160,166). Read as a draft, it could never be cleared, so every chat or queued send to a fresh Accept-edits session failed after about 6 s with composerNotCleared (issue #349). The hint is empty input when it spells a known mode hint, whole or cut with `...`, or the `<Name> mode: … (shift+tab to cycle)` shape of a newer mode, and every character carries an explicit colour: typed text is drawn in the terminal's default colour, so a draft with the same words stays a draft.
fn is_antigravity_mode_hint(text: &str, body: &[&(char, Style)]) -> bool {
    let text = text.trim();
    let spelled = ANTIGRAVITY_MODE_HINTS.iter().any(|hint| {
        *hint == text
            || text
                .strip_suffix("...")
                .or_else(|| text.strip_suffix('…'))
                .is_some_and(|cut| !cut.trim().is_empty() && hint.starts_with(cut))
    }) || (text.ends_with("(shift+tab to cycle)")
        && text
            .split_once(" mode: ")
            .is_some_and(|(name, _)| !name.is_empty() && !name.contains(' ')));
    spelled
        && !body.is_empty()
        && body
            .iter()
            .all(|(_, style)| style.foreground_index.is_some() || style.foreground_rgb.is_some())
}

/// CDXC:AgentScreenDetection 2026-11-04 WHY:
/// While `/ide` is connected, Claude Code (2.1.312, read from the shipped binary's `leadingPill`) draws the editor's context at the start of its input row, before any typed text: `❯ [⧉ In main.rs] ` for the open file, `❯ [⧉ 3 lines selected] ` or `[⧉ 3 lines from main.rs] ` for a selection, the label cut in the middle with `…` past 32 columns. Kill-line keys cannot remove it (only Backspace on an empty box dismisses it), so read as a draft it made every chat send fail with composerNotCleared and restart the agent for nothing (issue #406). It is not typed text: both Claude readers skip it, and the styled reader also requires its explicit `ide` colour, since typed text keeps the terminal's default colour.
/// Returns the pill's length in chars when `text` (the row after the `❯`, leading spaces removed) starts with one.
fn claude_ide_pill_len(text: &str) -> Option<usize> {
    let label = text.strip_prefix("[⧉ ")?;
    let close = label.find(']')?;
    let after = &label[close + 1..];
    let inner = &label[..close];
    let is_file = inner.starts_with("In ");
    let is_selection = inner
        .split_once(' ')
        .is_some_and(|(count, _)| !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()));
    ((is_file || is_selection) && (after.is_empty() || after.starts_with(' ')))
        .then(|| "[⧉ ".chars().count() + inner.chars().count() + 1)
}

/// The char range of Claude's IDE pill in `chars` (a row after its marker), when it is drawn in a
/// colour of its own.
fn claude_ide_pill(chars: &[(char, Style)]) -> Option<Range<usize>> {
    let start = chars.iter().position(|(ch, _)| !ch.is_whitespace())?;
    let text: String = chars[start..].iter().map(|(ch, _)| *ch).collect();
    let end = start + claude_ide_pill_len(&text)?;
    chars[start..end]
        .iter()
        .all(|(_, style)| style.foreground_index.is_some() || style.foreground_rgb.is_some())
        .then_some(start..end)
}

/// One row of Antigravity's active-items list: a status glyph, the task's start time in brackets,
/// then its command and state.
fn is_antigravity_active_item(line: &str) -> bool {
    let mut chars = line.trim_start().chars();
    if chars.next().is_none_or(|glyph| glyph.is_ascii()) {
        return false;
    }
    let Some(rest) = chars.as_str().strip_prefix(" [") else {
        return false;
    };
    let time = rest.as_bytes();
    time.len() > 9
        && time[8] == b']'
        && time[2] == b':'
        && time[5] == b':'
        && [0, 1, 3, 4, 6, 7]
            .iter()
            .all(|&index| time[index].is_ascii_digit())
}

/// CDXC:SessionChat 2026-09-11 WHY:
/// Clearing must read every logical row: Hermes can prefix its marker with a profile, Pi has no marker, and OMP paints its final input row inside the bottom corners.
/// Readiness uses these same regions so a populated multiline draft remains eligible for clearing.
pub(super) fn unmarked_rule_input_region(lines: &[String]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_titled_horizontal_rule(line))?;
    Some(head + 1..foot)
}

/// ZCode 3.11.2-24 uses Pi's unmarked editor, followed by its model/mode footer.
/// Require that footer immediately after the editor so a rule inside a picker cannot qualify.
/// CDXC:AgentScreenDetection 2026-10-16 WHY:
/// Only the footer's `◈ <model>` and `◉ <mode>` fields are required in ZCode's TUI; `⚡ <thinking level>` appears only when the model has one and is the first field dropped on a narrow pane. Requiring `⚡` made a ZCode 3.15.3 session on `account:zai-individual-coding-plan/GLM-5.3` (footer `◈ … ─ ◉ build ─ ctx 100% left ─ session 0 tokens`) read as having no input box, so every chat send was held forever while typing in the terminal worked.
pub(super) fn zcode_input_region(lines: &[String]) -> Option<Range<usize>> {
    let region = unmarked_rule_input_region(lines)?;
    let footer = lines.get(region.end + 1)?.trim();
    (footer.starts_with('◈')
        && footer.contains("◉")
        && lines[region.end + 2..]
            .iter()
            .all(|line| line.trim().is_empty()))
    .then_some(region)
}

/// CDXC:AgentScreenDetection 2026-10-04 WHY:
/// Freebuff 0.2.12 draws its input as the last rounded box above its footer (`<model> · <folder> · /model to change · Chat: …`, then `← for history · ? for help`), but its ads and its `ask_user` form use the same rounded box. The form puts a nested Submit box and an `↑↓ navigate` hint inside, so a box holding either is not the input.
pub(super) fn freebuff_input_region(lines: &[String]) -> Option<Range<usize>> {
    let help = lines
        .iter()
        .rposition(|line| line.contains("for history") && line.contains("for help"))?;
    let foot = lines[..help].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╰') && line.ends_with('╯')
    })?;
    if lines[foot + 1..help]
        .iter()
        .any(|line| line.trim_start().starts_with(['│', '╭', '╰']))
    {
        return None;
    }
    let head = lines[..foot].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╭') && line.ends_with('╮')
    })?;
    lines[head + 1..foot]
        .iter()
        .all(|line| {
            let line = line.trim();
            line.starts_with('│')
                && line.ends_with('│')
                && !line[3..].contains(['╭', '╰'])
                && !line.contains("↑↓ navigate")
        })
        .then_some(head + 1..foot)
}

#[path = "session_chat_dsh_composer.rs"]
pub(super) mod dsh_composer;

const FREEBUFF_PLACEHOLDERS: &[&str] = &[
    "enter a coding task or / for commands",
    "enter a coding task",
    "add to the current task (/ for commands)",
    "add to the current task",
    "ctrl-c to cancel queued messages",
    "enter bash command...",
    "describe a feature/bug or other request to be fleshed out...",
    "add instructions for this skill, or press enter to run it as-is...",
    "describe what you want to plan...",
    "describe what to review...",
    "enter image path or ctrl+v to paste",
];

/// The typed text of Freebuff's input box. An empty input shows the cursor `▍` followed by a
/// dim placeholder; typed text shows the cursor where it was left.
fn freebuff_composer_input(lines: &[String]) -> Option<SessionChatComposerInput> {
    let region = freebuff_input_region(lines)?;
    let rows: Vec<String> = lines[region.clone()]
        .iter()
        .map(|line| box_interior(line).to_string())
        .collect();
    let first = rows.iter().position(|row| !row.is_empty());
    let last = rows.iter().rposition(|row| !row.is_empty());
    let rows = match (first, last) {
        (Some(first), Some(last)) => &rows[first..=last],
        _ => &rows[..0],
    };
    let placeholder = rows.len() == 1
        && rows[0].starts_with('▍')
        && FREEBUFF_PLACEHOLDERS
            .contains(&rows[0]['▍'.len_utf8()..].trim().to_lowercase().as_str());
    let text = if placeholder {
        String::new()
    } else {
        rows.iter()
            .map(|row| row.replace('▍', ""))
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    };
    Some(SessionChatComposerInput {
        text,
        rows: region.len(),
        shell_mode: false,
        placeholder,
        attachments: 0,
        text_unreadable: false,
    })
}

/// Empryo's prompt glyph while no turn runs: `◈` focused, `◇` unfocused.
const EMPRYO_IDLE_MARKERS: &[char] = &['◈', '◇'];
/// The spinner frames Empryo draws in place of the prompt glyph while a turn runs.
const EMPRYO_BUSY_MARKERS: &[char] = &['◍', '◉', '◎'];
/// Rows Empryo paints below its input box: the statusline, with slack for a wrapped one.
const EMPRYO_FOOTER_MAX_LINES: usize = 3;

/// CDXC:AgentScreenDetection 2026-10-06 WHY:
/// Empryo 3.9.0-beta draws its input as the lowest rounded box on screen with only its statusline below, the first row opening with its prompt glyph (`◈`/`◇`, or a `◍◉◎` spinner frame while a turn runs; `$_n` and the input-box marker in its TUI bundle). Its question, approval, slash-command and queued-message panels are rounded boxes too, but they sit above the input box, and a question squeezes the input box to its top border, so the input exists only while that box is whole and last. Attached images are small boxes drawn inside it above the glyph row, so the input starts at that row.
pub(super) fn empryo_input_region(lines: &[String]) -> Option<Range<usize>> {
    empryo_input_box(lines).map(|(_, region)| region)
}

/// The row of Empryo's input box top border, which Empryo 3.9.1-beta draws its model on.
pub(crate) fn empryo_input_head(lines: &[String]) -> Option<usize> {
    empryo_input_box(lines).map(|(head, _)| head)
}

/// [`empryo_input_region`] with the row of the box's top border.
fn empryo_input_box(lines: &[String]) -> Option<(usize, Range<usize>)> {
    let foot = lines.iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╰') && line.ends_with('╯')
    })?;
    let mut footer = lines[foot + 1..]
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty());
    if footer
        .by_ref()
        .take(EMPRYO_FOOTER_MAX_LINES)
        .any(|line| line.starts_with(['╭', '│', '╰']))
        || footer.next().is_some()
    {
        return None;
    }
    let head = lines[..foot].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╭') && line.ends_with('╮')
    })?;
    if head + 1 == foot
        || !lines[head + 1..foot].iter().all(|line| {
            let line = line.trim();
            line.starts_with('│') && line.ends_with('│')
        })
    {
        return None;
    }
    let body = head + 1 + empryo_box_panel_rows(&lines[head + 1..foot])?;
    let start = (body..foot).find(|&row| empryo_marker(&lines[row]).is_some())?;
    lines[body..start]
        .iter()
        .all(|line| empryo_attachment_row(line).is_some())
        .then_some((head, start..foot))
}

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// Empryo 3.9.1-beta draws panels inside its input box above the prompt glyph, each closed off by a rule row: the tray after a turn (`│ ▸ Events 1`, `▾ 1 queued` with its list) and the slash-command list while a `/` command is typed. The rows those panels take at the top of `rows` (the box's interior), up to the last rule row above the prompt glyph row; `None` when no row holds the glyph.
fn empryo_box_panel_rows(rows: &[String]) -> Option<usize> {
    let glyph = rows.iter().rposition(|row| empryo_marker(row).is_some())?;
    Some(
        rows[..glyph]
            .iter()
            .rposition(|row| {
                let row = box_interior(row);
                !row.is_empty() && row.chars().all(|ch| ch == '\u{2500}')
            })
            .map_or(0, |rule| rule + 1),
    )
}

/// A box row's text between its `│` borders, trimmed.
fn box_interior(line: &str) -> &str {
    let line = line.trim();
    let line = line.strip_prefix('\u{2502}').unwrap_or(line);
    line.strip_suffix('\u{2502}').unwrap_or(line).trim()
}

/// The attachment boxes a row inside Empryo's input box draws (`│ ╭──╮ ╭──╮   │`), or `None`
/// for a row that is not part of them.
fn empryo_attachment_row(line: &str) -> Option<usize> {
    let inner = line.trim().strip_prefix('│')?.trim_start();
    inner
        .starts_with(['╭', '│', '╰'])
        .then(|| inner.matches('╭').count())
}

/// Empryo's voice button: a Nerd Font microphone, `♩` without a Nerd Font.
fn is_empryo_voice_button(ch: char) -> bool {
    ch == '\u{2669}' || crate::session_chat_options::is_nerd_font_icon(ch)
}

fn is_empryo_marker(ch: &char) -> bool {
    EMPRYO_IDLE_MARKERS.contains(ch) || EMPRYO_BUSY_MARKERS.contains(ch)
}

/// The prompt glyph opening `row`, the text right after an input-box border `│`.
///
/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// Empryo 3.9.1-beta draws its voice button before the glyph while voice input is on (`│  \u{f130}  ◈`, `♩` without a Nerd Font). Requiring the glyph right after the border read every 3.9.1 input box as missing, so chat sends waited forever and the draft handoff failed (Sven's try-it, 2026-10-07).
fn empryo_marker_after_border(row: &str) -> Option<char> {
    let row = row.strip_prefix(' ')?;
    let row = match row.trim_start().strip_prefix(is_empryo_voice_button) {
        Some(rest) if rest.starts_with(' ') => rest.trim_start(),
        _ => row,
    };
    let mut chars = row.chars();
    let marker = chars.next().filter(is_empryo_marker)?;
    matches!(chars.next(), None | Some(' ')).then_some(marker)
}

fn empryo_marker(line: &str) -> Option<char> {
    empryo_marker_after_border(line.trim().strip_prefix('│')?)
}

/// Whether Empryo's input box shows a running turn, or `None` when the box is not on screen.
pub(crate) fn empryo_composer_busy(screen_text: &str) -> Option<bool> {
    let lines: Vec<String> = screen_text.lines().map(strip_ansi_sgr).collect();
    let region = empryo_input_region(&lines)?;
    empryo_marker(&lines[region.start]).map(|marker| EMPRYO_BUSY_MARKERS.contains(&marker))
}

/// Whether an Empryo panel has the keyboard: every panel (`/router`, `/models`, `/settings`,
/// `/effort`, `/git`, the Ctrl+K palette) leaves the input box drawn with its unfocused glyph `◇`,
/// sometimes beside a side panel that cuts its right border off.
pub(crate) fn empryo_input_unfocused(screen_text: &str) -> bool {
    screen_text
        .lines()
        .rev()
        .map(strip_ansi_sgr)
        .find_map(|line| {
            let (_, row) = line.split_once('│')?;
            empryo_marker_after_border(row)
        })
        == Some('◇')
}

/// The cursor a VT capture ends on (`ESC[row;colH`), as a 0-based line of `screen.lines()` and a
/// 0-based column. The row counts from the top of the live screen, which follows the last clear
/// when the capture carries scrollback before it.
fn vt_capture_cursor(screen: &str) -> Option<(usize, usize)> {
    let (row, col) = screen.rmatch_indices("\u{1b}[").find_map(|(at, _)| {
        let rest = &screen[at + 2..];
        let end = rest.find(|ch: char| !ch.is_ascii_digit() && ch != ';')?;
        let (row, col) = rest[..end].split_once(';')?;
        rest[end..].starts_with('H').then_some(())?;
        Some((row.parse::<usize>().ok()?, col.parse::<usize>().ok()?))
    })?;
    let top = screen
        .rfind("\u{1b}[2J")
        .map_or(0, |clear| screen[..clear].matches('\n').count());
    Some((top + row.checked_sub(1)?, col.checked_sub(1)?))
}

/// CDXC:AgentScreenDetection 2026-10-06 WHY:
/// Empryo's empty input shows a rotating tip in its theme's muted color, and typed text differs from it only by color, which every theme sets differently. The cursor is the theme-free witness: it rests on the first input cell exactly when the input is empty. wmx's styled-row capture carries no cursor, so text there is marked unreadable: placing a Chat draft (which clears the box) and the attached-image clear stop with an error instead of wiping it, and carrying a terminal draft to Chat carries nothing and clears nothing, which keeps it in the terminal.
fn empryo_composer_input(screen: &str, lines: &[StyledLine]) -> Option<SessionChatComposerInput> {
    let plain: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
    let region = empryo_input_region(&plain)?;
    let marker = lines[region.start]
        .chars
        .iter()
        .position(|(ch, _)| is_empryo_marker(ch))?;
    let input_column = marker + 2;
    let rows: Vec<String> = lines[region.clone()]
        .iter()
        .map(|line| {
            let end = line
                .chars
                .iter()
                .rposition(|(ch, _)| *ch == '│')
                .unwrap_or(line.chars.len());
            line.chars[input_column.min(end)..end]
                .iter()
                .map(|(ch, _)| *ch)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    let attachments = plain[..region.start]
        .iter()
        .rev()
        .map_while(|line| empryo_attachment_row(line))
        .sum();
    let text = rows.join("\n").trim().to_string();
    let cursor = vt_capture_cursor(screen);
    Some(SessionChatComposerInput {
        text_unreadable: cursor.is_none() && !text.is_empty(),
        text,
        rows: region.len(),
        shell_mode: false,
        placeholder: cursor.is_none_or(|cursor| cursor == (region.start, input_column)),
        attachments,
    })
}

/// The tabs Empryo's tab bar shows (`▏  ALPHA output ✕▕  ▏  TAB-1 ✕▕   +`, drawn once a window
/// holds two or more tabs), as the number of tabs and the index of the active one, read from a
/// VT capture. `None` while no tab bar is on screen.
///
/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// Empryo 3.9.1-beta paints the active tab's label bold and every other label in its plain tab color; a tab's number badge, when shown, is bold in every tab, so only the label's letters tell the active tab, in every theme.
pub(crate) fn empryo_tab_bar(screen: &str) -> Option<(usize, Option<usize>)> {
    styled_lines(screen).iter().find_map(|line| {
        let mut tabs = 0;
        let mut active = None;
        let mut cell: Option<Vec<(char, Style)>> = None;
        for &(ch, style) in &line.chars {
            match (ch, cell.as_mut()) {
                ('\u{258f}', _) => cell = Some(Vec::new()),
                ('\u{2595}', Some(chars)) => {
                    if chars.iter().rev().find(|(ch, _)| !ch.is_whitespace())?.0 != '\u{2715}' {
                        return None;
                    }
                    if chars
                        .iter()
                        .any(|(ch, style)| ch.is_alphabetic() && style.bold)
                    {
                        active = Some(tabs);
                    }
                    tabs += 1;
                    cell = None;
                }
                (_, Some(chars)) => chars.push((ch, style)),
                (_, None) => {}
            }
        }
        (tabs > 0).then_some((tabs, active))
    })
}

pub(super) fn hermes_input_region(lines: &[String]) -> Option<Range<usize>> {
    let region = unmarked_rule_input_region(lines)?;
    let start = region.clone().find(|&i| !lines[i].trim().is_empty())?;
    super::is_profiled_marker_line(&lines[start], '❯').then_some(start..region.end)
}

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's box composer draws its statusline into the top border, and that line changes with the symbol preset (`π >` with Unicode symbols, `󰵗` and powerline glyphs with Nerd Font) and turns into a spinner mid-turn. Requiring `π` and `>` there read a Nerd Font OMP as never ready, so its first chat message waited in the queue forever.
/// The frame is the signature instead: OMP merges the input's last row into the foot (`╰─ text ─╯`), while its dialogs and welcome card close with a solid box rule.
pub(super) fn omp_input_region(lines: &[String]) -> Option<Range<usize>> {
    omp_input(lines).map(|(region, _)| region)
}

/// OMP's `chip.paste` symbol in its Unicode, Nerd Font and ASCII symbol presets.
const OMP_PASTE_CHIP_GLYPHS: &[&str] = &["\u{1f4c4}", "\u{f15c}", "txt"];

/// CDXC:Clipboard 2026-10-18 WHY:
/// OMP 18.11 keeps a long paste in its editor as `[Paste #N, +M lines]` but draws it in the input box as a `<chip.paste> #N` chip (`📄 #1`), with the paste's first lines in a separate box above the input. The input box showed neither the pasted text nor the `[Paste` spelling paste verification looked for, so every OMP message over ten lines failed as "The terminal did not accept the pasted message", was cleared with Ctrl+C and retried until the row failed: Send Bug / Feedback's brief (GitHub #254) and an orchestrator's first brief to an OMP thread. A chip in the input box is OMP's proof that the paste landed.
pub(crate) fn omp_paste_chip_shown(input_text: &str) -> bool {
    let words: Vec<&str> = input_text.split_whitespace().collect();
    words.windows(2).any(|pair| {
        OMP_PASTE_CHIP_GLYPHS.contains(&pair[0])
            && pair[1]
                .strip_prefix('#')
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    })
}

/// The `composer.shape` that drew OMP's input, which decides how each row's chrome is stripped.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OmpShape {
    Box,
    Band,
    /// `claude`: `❯ ` and two-space rows between two rules.
    MarkedSandwich,
    /// `pi`: one-space padded rows between two rules.
    PaddedSandwich,
    /// `rule` (under a top rule) and `borderless`: `❯ ` and two-space rows.
    Gutter,
    /// `field`: `▐ … ▌` rows.
    Field,
    /// `rail`: `▎ …` rows.
    Rail,
}

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// OMP's setup wizard asks every new user to pick one of eight composer shapes, and Ghostex read only the box and the band, so a user who picked any other one could never send from chat. Each shape is read by its own row grammar (measured against OMP 18.8.2's `packages/tui/src/components/composer/`), tried in an order where a looser grammar cannot claim a stricter shape's rows.
/// Every shape but the box and the band ends in OMP's status bar, and while the slash-command list is open the list replaces the status bar. So an input counts only when nothing but those follows it.
fn omp_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    if let Some(region) = omp_box_input_region(lines) {
        return Some((region, OmpShape::Box));
    }
    if let Some(region) = omp_band_input_region(lines) {
        return Some((region, OmpShape::Band));
    }
    omp_sandwich_input(lines)
        .or_else(|| omp_capped_input(lines, '▐', OmpShape::Field))
        .or_else(|| omp_capped_input(lines, '▎', OmpShape::Rail))
        .or_else(|| omp_gutter_input(lines))
}

/// Rows OMP may draw under its input: the one-line status bar, or the slash-command list (two-space rows with one selected `❯ ` row), which hides the status bar.
fn omp_composer_tail(rows: &[String]) -> bool {
    let rows: Vec<_> = rows.iter().filter(|row| !row.trim().is_empty()).collect();
    let status_bar = rows.len() <= 1
        && rows
            .iter()
            .all(|row| !row.trim_start().starts_with(['╭', '╰', '│', '❯']));
    status_bar || omp_slash_list(&rows)
}

/// OMP's slash-command list: two-space rows with one selected `❯ ` row, drawn under every shape's input.
fn omp_slash_list(rows: &[&String]) -> bool {
    !rows.is_empty()
        && rows
            .iter()
            .all(|row| row.starts_with("❯ ") || row.starts_with("  "))
        && rows.iter().filter(|row| row.starts_with('❯')).count() == 1
}

/// The input's rows from `start`: following rows that `continues` accepts, blank rows included, ending at the last row with text.
fn omp_rows(lines: &[String], start: usize, continues: impl Fn(&str) -> bool) -> Range<usize> {
    let rows = lines[start + 1..]
        .iter()
        .take_while(|line| continues(line) || line.trim().is_empty())
        .count();
    let end = (start + 1..=start + rows)
        .rfind(|&row| !lines[row].trim().is_empty())
        .unwrap_or(start);
    start..end + 1
}

fn omp_marked_row(line: &str) -> bool {
    line.starts_with("❯ ") || line.trim_end() == "❯"
}

/// `claude` and `pi` shapes: the input sits between two full-width rules (the top one can carry a status chip), with the status bar or the slash list under the foot.
fn omp_sandwich_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    if !omp_composer_tail(&lines[foot + 1..]) {
        return None;
    }
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_horizontal_rule(line) || is_titled_horizontal_rule(line))?;
    let rows = &lines[head + 1..foot];
    if rows.is_empty()
        || rows
            .iter()
            .any(|row| row.trim_start().starts_with(['╭', '╰', '│']))
    {
        return None;
    }
    let region = head + 1..foot;
    if omp_marked_row(&rows[0])
        && rows[1..]
            .iter()
            .all(|row| row.starts_with("  ") || row.trim().is_empty())
    {
        Some((region, OmpShape::MarkedSandwich))
    } else if rows
        .iter()
        .all(|row| row.starts_with(' ') || row.trim().is_empty())
    {
        Some((region, OmpShape::PaddedSandwich))
    } else {
        None
    }
}

/// `field` and `rail` shapes: every input row, blank ones included, starts with the shape's accent cap.
fn omp_capped_input(
    lines: &[String],
    cap: char,
    shape: OmpShape,
) -> Option<(Range<usize>, OmpShape)> {
    let end = lines.iter().rposition(|line| line.starts_with(cap))?;
    let start = lines[..end]
        .iter()
        .rposition(|line| !line.starts_with(cap))
        .map_or(0, |row| row + 1);
    omp_composer_tail(&lines[end + 1..]).then_some((start..end + 1, shape))
}

/// `rule` and `borderless` shapes: a `❯ ` row then two-space rows, above the status bar or the slash list. The slash list draws the same `❯ ` and two-space rows, so the input is the topmost `❯ ` row of the trailing gutter block whose rows are followed only by that tail.
fn omp_gutter_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    let gutter = |line: &str| omp_marked_row(line) || line.starts_with("  ");
    let last = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let bottom = if gutter(&lines[last]) { last + 1 } else { last };
    let top = lines[..bottom]
        .iter()
        .rposition(|line| !gutter(line) && !line.trim().is_empty())
        .map_or(0, |row| row + 1);
    (top..bottom)
        .filter(|&row| omp_marked_row(&lines[row]))
        .map(|row| omp_rows(lines, row, |line| line.starts_with("  ")))
        .find(|region| omp_composer_tail(&lines[region.end..]))
        .map(|region| (region, OmpShape::Gutter))
}

/// One input row's text with the shape's chrome removed.
fn omp_row_text(shape: OmpShape, first: bool, line: &str) -> String {
    let line = match shape {
        OmpShape::Box => {
            // OMP merges its final input row into ╰─ text ─╯.
            let line = line.trim();
            let inner = line
                .chars()
                .skip(1)
                .take(line.chars().count().saturating_sub(2))
                .collect::<String>();
            return if line.starts_with('╰') {
                let inner = inner.strip_prefix('─').unwrap_or(&inner);
                inner.strip_suffix('─').unwrap_or(inner).to_string()
            } else {
                inner
            };
        }
        // The band's first row carries the `╰─ ` cue, later rows a three-space indent.
        OmpShape::Band => match line.strip_prefix(OMP_BAND_CUE) {
            Some(row) => row.strip_prefix(' ').unwrap_or(row),
            None => line.strip_prefix("   ").unwrap_or(line),
        },
        OmpShape::MarkedSandwich | OmpShape::Gutter => match line.strip_prefix('❯') {
            Some(row) if first => row.strip_prefix(' ').unwrap_or(row),
            _ => line.strip_prefix("  ").unwrap_or(line),
        },
        OmpShape::PaddedSandwich => line.strip_prefix(' ').unwrap_or(line),
        OmpShape::Field | OmpShape::Rail => {
            let row = line.trim_start_matches(['▐', '▎']);
            let row = row.strip_prefix(' ').unwrap_or(row).trim_end();
            row.strip_suffix(['▌', '█']).unwrap_or(row)
        }
    };
    line.trim_end().to_string()
}

fn omp_box_input_region(lines: &[String]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╰') && line.ends_with('╯')
    })?;
    let tail: Vec<_> = lines[foot + 1..]
        .iter()
        .filter(|row| !row.trim().is_empty())
        .collect();
    if !tail.is_empty() && !omp_slash_list(&tail) {
        return None;
    }
    let interior = lines[foot].trim().strip_prefix('╰')?.strip_suffix('╯')?;
    if !interior.starts_with('─')
        || interior
            .chars()
            .all(|c| ('\u{2500}'..='\u{257f}').contains(&c))
    {
        return None;
    }
    let head = lines[..foot].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╭') && line.ends_with('╮')
    })?;
    if !lines[head + 1..foot].iter().all(|line| {
        let line = line.trim();
        line.starts_with('│') && (line.ends_with('│') || line.ends_with('█'))
    }) {
        return None;
    }
    Some(head + 1..foot + 1)
}

/// The cue OMP's status band composer draws at column 0 of the input's first row; later rows are indented three spaces.
const OMP_BAND_CUE: &str = "╰─";

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// OMP 18.0.10 made the status band its default composer shape (`composer.shape: band`, also offered first by its setup wizard): the statusline is a flush row above an unframed input whose first row starts with `╰─ ` and has no `╭` head or `╯` corner. Reading only the rounded box called every default OMP "not on screen yet", so chat messages never reached its terminal and users pasted them in by hand.
/// The cue is OMP's own literal gutter, not a themed glyph, and its overlays and tool frames close with `╰───╯` rules, so a `╰─` row whose rest is not a rule, with no frame drawn under it, is the live input. The row above it is the band (empty while the statusline starts). The slash-command list OMP opens under the input is not indented, so the input ends at the last indented row.
fn omp_band_input_region(lines: &[String]) -> Option<Range<usize>> {
    let last = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let cue = lines[..=last]
        .iter()
        .rposition(|line| line.starts_with(OMP_BAND_CUE))?;
    let rest = lines[cue][OMP_BAND_CUE.len()..].trim_end();
    if cue == 0
        || rest.starts_with(|c| ('\u{2500}'..='\u{257f}').contains(&c))
        || rest.ends_with('╯')
        || lines[cue + 1..=last]
            .iter()
            .any(|line| line.trim_start().starts_with(['╭', '╰', '│']))
    {
        return None;
    }
    let rows = lines[cue + 1..=last]
        .iter()
        .take_while(|line| line.starts_with("   ") || line.trim().is_empty())
        .count();
    let end = (cue + 1..=cue + rows)
        .rfind(|&row| !lines[row].trim().is_empty())
        .unwrap_or(cue);
    Some(cue..end + 1)
}

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's empty composer shows one right-aligned gesture hint in its foot (`╰─  ⇧⇥ to change thinking effort ─╯`, or `󰘶 󰌒 …` with Nerd Font symbols): the key as one accent-colored span, the label italic. Reading it as a draft failed every send with "could not be cleared". Typed input is never italic, so an italic label with only one key span before it is the hint.
fn omp_hint_only(line: &StyledLine) -> bool {
    // Box-drawing and block glyphs frame every shape's row, and `❯` is the gutter of the `claude`, `rule` and `borderless` shapes.
    let is_chrome =
        |ch: char| ch.is_whitespace() || ('\u{2500}'..='\u{259f}').contains(&ch) || ch == '❯';
    let Some(label) = line
        .chars
        .iter()
        .position(|(ch, style)| style.italic && !is_chrome(*ch))
    else {
        return false;
    };
    let mut key = line.chars[..label]
        .iter()
        .filter(|(ch, _)| !is_chrome(*ch))
        .map(|(_, style)| (style.foreground_rgb, style.foreground_index));
    let key_color = key.next();
    key.all(|color| Some(color) == key_color)
        && line.chars[label..]
            .iter()
            .all(|(ch, style)| style.italic || is_chrome(*ch))
}

/// CDXC:AgentScreenDetection 2026-09-09 WHY:
/// Cursor 2026.09.08 renders either half-block borders or a background-filled input with blank padding, depending on terminal capabilities.
/// The borderless layout must have its model/usage footer and context footer below the input, with no open menu after them; an arrow alone also appears in pickers.
pub(super) fn cursor_input_region(lines: &[String]) -> Option<Range<usize>> {
    if let Some(foot) = lines
        .iter()
        .rposition(|line| super::is_frame_rule(line, '\u{2580}', 9, 10))
    {
        if let Some(head) = lines[..foot]
            .iter()
            .rposition(|line| super::is_frame_rule(line, '\u{2584}', 9, 10))
        {
            let start = (head + 1..foot).find(|&i| lines[i].trim_start().starts_with('→'))?;
            return Some(start..foot);
        }
    }
    let footer = lines
        .iter()
        .rposition(|line| crate::session_chat_options::match_cursor_statusline(line).is_some())?;
    let tail: Vec<_> = lines[footer + 1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if tail.len() != 1 || !tail[0].contains("Ctx ") || !tail[0].trim_end().ends_with("% used") {
        return None;
    }
    let start = lines[..footer]
        .iter()
        .rposition(|line| line.trim_start().starts_with('→'))?;
    if start == 0 || !lines[start - 1].trim().is_empty() || !lines[footer - 1].trim().is_empty() {
        return None;
    }
    let end = (start + 1..footer)
        .rfind(|&i| !lines[i].trim().is_empty())
        .map_or(start + 1, |i| i + 1);
    Some(start..end)
}

pub fn claude_composer_draft(screen: &str) -> Option<String> {
    if super::is_claude_code_agents_screen(screen) {
        return None;
    }
    let lines: Vec<_> = screen.lines().map(strip_ansi_sgr).collect();
    let region = rule_input_region(&lines, CLAUDE_COMPOSER_MARKERS)?;
    let first = lines[region.start].trim_start();
    let text = first.strip_prefix('❯').unwrap_or(first).trim_start();
    // A plain capture has no colours, so the IDE pill is told by its shape alone here.
    let (shell, rest) = match text.strip_prefix('!') {
        Some(rest) => ("!", rest.trim_start()),
        None => ("", text),
    };
    let text = match claude_ide_pill_len(rest) {
        Some(len) => format!(
            "{shell} {}",
            rest.char_indices()
                .nth(len)
                .map_or("", |(at, _)| &rest[at..])
        ),
        None => text.to_string(),
    };
    Some(
        std::iter::once(text.trim())
            .chain(
                lines[region.start + 1..region.end]
                    .iter()
                    .map(|line| line.trim()),
            )
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[derive(Debug)]
pub struct SessionChatComposerInput {
    pub text: String,
    pub rows: usize,
    /// An empty shell editor still needs to return to normal mode before replacement.
    pub shell_mode: bool,
    placeholder: bool,
    /// Text is on screen but the capture cannot tell a tip from a typed draft (Empryo without a
    /// cursor); `text_is_empty` still reports it empty.
    text_unreadable: bool,
    /// Images attached in the input box (Empryo draws each as a box above its text).
    attachments: usize,
}

impl SessionChatComposerInput {
    pub fn is_empty(&self) -> bool {
        self.attachments == 0 && self.text_is_empty()
    }

    /// No typed text, whatever images are attached.
    pub(crate) fn text_is_empty(&self) -> bool {
        self.text.trim().is_empty()
            || self.placeholder
            || (self.shell_mode && self.text.trim() == "!")
    }

    pub(crate) fn attachments(&self) -> usize {
        self.attachments
    }

    pub(crate) fn text_unreadable(&self) -> bool {
        self.text_unreadable
    }
}

#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    faint: bool,
    italic: bool,
    inverse: bool,
    underline: bool,
    blink: bool,
    hidden: bool,
    crossed_out: bool,
    foreground_rgb: Option<[u16; 3]>,
    foreground_index: Option<u16>,
    background_rgb: Option<[u16; 3]>,
}

struct StyledLine {
    chars: Vec<(char, Style)>,
    text: String,
}

/// CDXC:SessionChat 2026-09-08 WHY:
/// Plain captures make Codex's empty placeholder indistinguishable from a draft. VT captures preserve its faint style and distinguish the live bold prompt from dim transcript echoes, without depending on placeholder wording.
fn styled_lines(screen: &str) -> Vec<StyledLine> {
    let mut style = Style::default();
    screen
        .lines()
        .map(|line| {
            let mut chars = line.chars().peekable();
            let mut visible = Vec::new();
            while let Some(ch) = chars.next() {
                if ch != '\u{1b}' {
                    visible.push((ch, style));
                    continue;
                }
                if chars.next() != Some('[') {
                    continue;
                }
                let mut parameters = String::new();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        if next == 'm' {
                            let values: Vec<_> = parameters
                                .split(';')
                                .map(|p| p.parse::<u16>().unwrap_or(0))
                                .collect();
                            let mut index = 0;
                            while index < values.len() {
                                match values[index] {
                                    0 => style = Style::default(),
                                    1 => style.bold = true,
                                    2 => style.faint = true,
                                    3 => style.italic = true,
                                    23 => style.italic = false,
                                    4 | 21 => style.underline = true,
                                    24 => style.underline = false,
                                    5 | 6 => style.blink = true,
                                    25 => style.blink = false,
                                    8 => style.hidden = true,
                                    28 => style.hidden = false,
                                    9 => style.crossed_out = true,
                                    29 => style.crossed_out = false,
                                    7 => style.inverse = true,
                                    27 => style.inverse = false,
                                    30..=37 | 39 | 90..=97 => {
                                        style.foreground_rgb = None;
                                        style.foreground_index = match values[index] {
                                            30..=37 => Some(values[index] - 30),
                                            90..=97 => Some(values[index] - 90 + 8),
                                            _ => None,
                                        };
                                    }
                                    40..=47 | 49 | 100..=107 => style.background_rgb = None,
                                    22 => {
                                        style.bold = false;
                                        style.faint = false;
                                    }
                                    38 | 48 | 58 => {
                                        let rgb = if values.get(index + 1) == Some(&2) {
                                            values
                                                .get(index + 2..index + 5)
                                                .and_then(|rgb| rgb.try_into().ok())
                                        } else {
                                            None
                                        };
                                        match values[index] {
                                            38 => {
                                                style.foreground_rgb = rgb;
                                                style.foreground_index =
                                                    if values.get(index + 1) == Some(&5) {
                                                        values.get(index + 2).copied()
                                                    } else {
                                                        None
                                                    };
                                            }
                                            48 => style.background_rgb = rgb,
                                            _ => {}
                                        }
                                        index += match values.get(index + 1) {
                                            Some(2) => 4,
                                            Some(5) => 2,
                                            _ => 0,
                                        };
                                    }
                                    _ => {}
                                }
                                index += 1;
                            }
                        }
                        break;
                    }
                    parameters.push(next);
                }
            }
            StyledLine {
                text: visible.iter().map(|(ch, _)| *ch).collect(),
                chars: visible,
            }
        })
        .collect()
}

/// CDXC:AgentScreenDetection 2026-09-14 WHY:
/// Codex's sparkle.rs blends eight single-dot Braille glyphs with the terminal palette, so light and tinted themes are not grayscale. Stars have explicit RGB foreground/background and no text modifiers; typed Braille inherits the default foreground.
/// Effort animations tint columns independently, so a star's background need not equal the prompt's background.
fn clear_codex_composer_particles(lines: &mut [StyledLine]) {
    let Some(start) = codex_composer_start(lines) else {
        return;
    };
    let Some(background) = lines[start].chars[0].1.background_rgb else {
        return;
    };
    let start = (0..start)
        .rev()
        .find(|&i| {
            lines[i]
                .chars
                .first()
                .is_none_or(|(_, style)| style.background_rgb != Some(background))
        })
        .map_or(0, |i| i + 1);
    for line in &mut lines[start..] {
        if line
            .chars
            .first()
            .is_none_or(|(_, style)| style.background_rgb != Some(background))
        {
            break;
        }
        for (ch, style) in &mut line.chars {
            if matches!(ch, '⠁' | '⠂' | '⠄' | '⠈' | '⠐' | '⠠' | '⡀' | '⢀')
                && !style.bold
                && !style.faint
                && !style.italic
                && !style.inverse
                && !style.underline
                && !style.blink
                && !style.hidden
                && !style.crossed_out
                && style.background_rgb.is_some()
                && style.foreground_rgb.is_some()
            {
                *ch = ' ';
            }
        }
        line.text = line.chars.iter().map(|(ch, _)| *ch).collect();
    }
}

fn codex_composer_start(lines: &[StyledLine]) -> Option<usize> {
    // The live prefix is in column zero; continuation rows start after LIVE_PREFIX_COLS.
    // Stop at the newest prefix, including disabled and shell composers, rather than a transcript echo.
    let start = lines.iter().rposition(|line| {
        line.chars
            .first()
            .is_some_and(|(ch, _)| matches!(ch, '›' | '»' | '!'))
    })?;
    let (marker, style) = lines[start].chars[0];
    (marker != '!' && style.bold && !style.faint).then_some(start)
}

fn codex_input_region(lines: &[StyledLine]) -> Option<Range<usize>> {
    let start = codex_composer_start(lines)?;
    // Selection rows make both the marker and label bold. The textarea only adds bold with inverse for search matches, which can remain after accepting a Vim search.
    if lines[start].chars[1..]
        .iter()
        .find(|(ch, _)| !ch.is_whitespace())
        .is_some_and(|(_, style)| style.bold && !style.faint && !style.inverse)
    {
        return None;
    }
    let foot = if let Some(background) = lines[start].chars[0].1.background_rgb {
        // The composer owns one bottom padding row, even with no statusline or a multiline footer.
        let end = (start + 1..lines.len())
            .find(|&i| {
                lines[i]
                    .chars
                    .first()
                    .is_none_or(|(_, style)| style.background_rgb != Some(background))
            })
            .unwrap_or(lines.len());
        end.checked_sub(1).filter(|&foot| foot > start)?
    } else {
        let last = (start + 1..lines.len()).rfind(|&i| !lines[i].text.trim().is_empty())?;
        (start + 1..last).rfind(|&i| lines[i].text.trim().is_empty())?
    };
    if !lines[foot].text.trim().is_empty() {
        return None;
    }
    // History/Vim searches keep the draft visible but route input into a separate footer editor.
    if lines[foot + 1..].iter().any(|line| {
        let text = line.text.trim();
        text.starts_with("reverse-i-search:")
            || line
                .chars
                .iter()
                .find(|(ch, _)| !ch.is_whitespace())
                .is_some_and(|(ch, style)| {
                    matches!(ch, '/' | '?')
                        && !style.bold
                        && !style.faint
                        && style.foreground_index == Some(6)
                })
    }) {
        return None;
    }
    let body = &lines[start].text;
    if body.contains("Viewing sub-agent")
        && lines[start].chars[1..]
            .iter()
            .filter(|(ch, _)| !ch.is_whitespace())
            .all(|(_, style)| style.faint)
    {
        return None;
    }
    Some(start..foot)
}

/// Codex renders remote attachments above the marker, separated from its textarea by a blank row.
/// They must count as a draft or clear verification skips Ctrl+C and attaches old images to the next message.
fn codex_remote_images(lines: &[StyledLine], start: usize) -> Vec<String> {
    if start == 0 || !lines[start - 1].text.trim().is_empty() {
        return Vec::new();
    }
    let mut images: Vec<_> = lines[..start - 1]
        .iter()
        .rev()
        .take_while(|line| {
            let label = line.text.trim();
            label
                .strip_prefix("[Image #")
                .and_then(|label| label.strip_suffix(']'))
                .is_some_and(|number| {
                    !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
                })
                && line
                    .chars
                    .iter()
                    .filter(|(ch, _)| !ch.is_whitespace())
                    .all(|(_, style)| style.foreground_index == Some(6))
        })
        .map(|line| line.text.trim().to_string())
        .collect();
    images.reverse();
    images
}

/// Input only, excluding transcript and footer. Call with a VT capture when proving a draft empty.
pub fn session_chat_composer_input(agent: &str, screen: &str) -> Option<SessionChatComposerInput> {
    if matches!(agent, "claude" | "openclaude") && super::is_claude_code_agents_screen(screen) {
        return None;
    }
    if agent == "grok" {
        return super::grok_composer_draft(screen).map(|text| {
            // CDXC:AgentScreenDetection 2026-09-09 WHY: Grok's empty composer paints "Type a message..." in RGB 78,78,78 rather than SGR faint. Treating it as a draft held model selections forever; checking its VT style protects real input with the same words.
            // CDXC:AgentScreenDetection 2026-09-16 WHY:
            // Grok's unaccepted next-prompt suggestion is italic RGB 88,88,88. Treating it as typed input made verified clearing fail, blocking /compact and queued chat messages even though typing replaces the suggestion.
            let placeholder = styled_lines(screen)
                .iter()
                .rev()
                .find(|line| super::is_boxed_marker_line(&line.text, '❯'))
                .is_some_and(|line| {
                    let Some(marker) = line.chars.iter().position(|(ch, _)| *ch == '❯') else {
                        return false;
                    };
                    let Some(border) = line
                        .chars
                        .iter()
                        .rposition(|(ch, _)| *ch == '│')
                        .filter(|border| *border > marker)
                    else {
                        return false;
                    };
                    let input_chars: Vec<_> = line.chars[marker + 1..border]
                        .iter()
                        .filter(|(ch, _)| !ch.is_whitespace())
                        .collect();
                    !input_chars.is_empty()
                        && input_chars.iter().all(|(_, style)| {
                            (text == "Type a message..."
                                && style.foreground_rgb == Some([78, 78, 78]))
                                || (style.italic && style.foreground_rgb == Some([88, 88, 88]))
                        })
                });
            SessionChatComposerInput {
                text,
                rows: 1,
                shell_mode: false,
                placeholder,
                attachments: 0,
                text_unreadable: false,
            }
        });
    }
    if agent == "freebuff" {
        let lines: Vec<String> = screen.lines().map(strip_ansi_sgr).collect();
        return freebuff_composer_input(&lines);
    }
    if agent == "dsh" {
        let lines: Vec<String> = screen.lines().map(strip_ansi_sgr).collect();
        return dsh_composer::dsh_composer_input(&lines);
    }
    let mut lines = styled_lines(screen);
    if agent == "empryo" {
        return empryo_composer_input(screen, &lines);
    }
    if agent == "codex" {
        clear_codex_composer_particles(&mut lines);
    }
    let plain: Vec<_> = lines.iter().map(|line| line.text.clone()).collect();
    if matches!(agent, "pi" | "omp" | "zcode") {
        let (region, omp_shape) = if agent == "zcode" {
            (zcode_input_region(&plain)?, None)
        } else if agent == "pi" {
            (unmarked_rule_input_region(&plain)?, None)
        } else {
            let (region, shape) = omp_input(&plain)?;
            (region, Some(shape))
        };
        let text = plain[region.clone()]
            .iter()
            .enumerate()
            .map(|(index, line)| match omp_shape {
                Some(shape) => omp_row_text(shape, index == 0, line),
                // ZCode's editor pads each row with one space and fills it to the pane's width;
                // the draft moved into the chat is the text alone.
                None if agent == "zcode" => line
                    .strip_prefix(' ')
                    .unwrap_or(line)
                    .trim_end()
                    .to_string(),
                None => line.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Some(SessionChatComposerInput {
            text,
            rows: region.len(),
            shell_mode: false,
            placeholder: agent == "omp" && region.len() == 1 && omp_hint_only(&lines[region.start]),
            attachments: 0,
            text_unreadable: false,
        });
    }
    let region = match agent {
        "claude" | "openclaude" => rule_input_region(&plain, CLAUDE_COMPOSER_MARKERS)?,
        "antigravity" => antigravity_input_region(&plain)?,
        "cursor" => cursor_input_region(&plain)?,
        "hermes-agent" => hermes_input_region(&plain)?,
        // CDXC:AgentScreenDetection 2026-09-11 DECISION:
        // User: keep Codex 0.153 and earlier working alongside 0.154, with or without stars. The bold prompt and dim placeholder identify input independently of optional particles and background colors.
        "codex" => codex_input_region(&lines)?,
        _ => return None,
    };
    let first = &lines[region.start];
    let marker = first.chars.iter().position(|(ch, _)| {
        if agent == "hermes-agent" {
            *ch == '❯'
        } else {
            !ch.is_whitespace()
        }
    })?;
    let shell_mode = matches!(agent, "claude" | "openclaude") && first.chars[marker].0 == '!';
    let ide_pill = matches!(agent, "claude" | "openclaude")
        .then(|| claude_ide_pill(&first.chars[marker + 1..]))
        .flatten()
        .map(|pill| marker + 1 + pill.start..marker + 1 + pill.end);
    let outside_pill = |index: &usize| !ide_pill.as_ref().is_some_and(|pill| pill.contains(index));
    let body: Vec<_> = (marker + 1..first.chars.len())
        .filter(outside_pill)
        .map(|index| &first.chars[index])
        .chain(
            lines[region.start + 1..region.end]
                .iter()
                .flat_map(|line| line.chars.iter()),
        )
        .filter(|(ch, _)| !ch.is_whitespace())
        .collect();
    // The shell marker is part of the logical draft, including for paste and returned-prompt comparisons.
    let text_start = if shell_mode { marker } else { marker + 1 };
    let text = std::iter::once(
        (text_start..first.chars.len())
            .filter(outside_pill)
            .map(|index| first.chars[index].0)
            .collect::<String>(),
    )
    .chain(
        plain[region.start + 1..region.end]
            .iter()
            .map(|line| line.trim_end().to_string()),
    )
    .collect::<Vec<_>>()
    .join("\n")
    .trim()
    .to_string();
    // Cursor's caret inverts the first placeholder character while the rest stays faint.
    let placeholder = (!body.is_empty()
        && body.iter().enumerate().all(|(index, (_, style))| {
            style.faint
                // Hermes's prompt_toolkit placeholder is italic; editable input inherits the terminal style.
                || (agent == "hermes-agent" && style.italic)
                || (agent == "cursor" && index == 0 && style.inverse && body.len() > 1)
        })
        && !text.to_lowercase().contains("[paste"))
        || (agent == "antigravity" && is_antigravity_mode_hint(&text, &body));
    let mut input = SessionChatComposerInput {
        text,
        rows: region.len(),
        shell_mode,
        placeholder,
        attachments: 0,
        text_unreadable: false,
    };
    if agent == "codex" {
        let mut images = codex_remote_images(&lines, region.start);
        if !images.is_empty() {
            input.rows += images.len() + 1;
            if !input.placeholder && !input.text.is_empty() {
                images.push(input.text);
            }
            input.text = images.join("\n");
            input.placeholder = false;
        }
    }
    Some(input)
}

#[cfg(test)]
mod zcode_tests {
    use super::*;
    use crate::session_chat_composer::detect_session_chat_composer_ready;

    #[test]
    fn zcode_editor_accepts_multiline_drafts_but_not_open_pickers() {
        let screen = "Turn cancelled.\n────────────────────────────────────────\nfirst line\nsecond line\n────────────────────────────────────────\n ◈ zai/glm-5.2 ─ ◉ build ─ ⚡ max ─ ctx 100% left\n";
        let input = session_chat_composer_input("zcode", screen).unwrap();
        assert_eq!(input.text, "first line\nsecond line");
        assert_eq!(input.rows, 2);
        assert!(!detect_session_chat_composer_ready(Some("zcode"), screen)
            .blocks_message_for(Some("zcode")));
        for blocked in [
            String::new(),
            screen.replace("◈ zai", "other"),
            format!("{screen}Choose a model\n"),
        ] {
            assert!(detect_session_chat_composer_ready(Some("zcode"), &blocked)
                .blocks_message_for(Some("zcode")));
            assert!(session_chat_composer_input("zcode", &blocked).is_none());
        }
    }
}

#[cfg(test)]
mod claude_status_line_tests {
    use super::*;

    /// The bottom of session G14iv's screen on 2026-10-27 (200 columns, trimmed): a titled top
    /// rule and a statusline three rows tall, the last with Claude's own "← for agents · 2
    /// feedback drafts" notes.
    fn screen(input_rows: &[&str]) -> String {
        let rule = "─".repeat(120);
        let mut lines = vec![
            "✻ Churned for 6s · done 8:33 AM".to_string(),
            format!("{:>100}", "◉ xhigh · /effort"),
            format!("{} Mr Big Chungus ─", "─".repeat(100)),
        ];
        lines.extend(input_rows.iter().map(|row| row.to_string()));
        lines.push(rule);
        lines.push("  Opus 5.5 (1M) | xhigh | Ctx 57% | $757.85".to_string());
        lines.push("  ff8d2575-b069-47b5-b375-69750cc8353f | …/Ghostex".to_string());
        lines.push(
            "  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents · 2 feedback drafts"
                .to_string(),
        );
        lines.join("\n")
    }

    #[test]
    fn statusline_rows_under_the_input_box_are_never_draft_text() {
        assert_eq!(claude_composer_draft(&screen(&["❯ "])).as_deref(), Some(""));
        assert_eq!(
            crate::session_chat_composer::claude_composer_input_text(&screen(&["❯ "])),
            None
        );
        assert_eq!(
            claude_composer_draft(&screen(&["❯ first line", "  second line", "  ```"])).as_deref(),
            Some("first line second line ```")
        );
    }
}

#[cfg(test)]
mod antigravity_mode_hint_tests {
    use super::*;
    use crate::session_chat_composer::detect_session_chat_composer_ready;

    /// The issue #349 screen (Antigravity CLI 1.4.9, account and folder lines removed) with the
    /// styles a live 1.4.9 draws: `>` in 256-colour 111, the hint in 109, typed text in the
    /// default colour.
    fn screen(input_row: &str) -> String {
        let rule = format!("\u{1b}[38;5;238m{}", "─".repeat(55));
        [
            "  Antigravity CLI 1.4.9",
            "  Gemini 3.8 Flash (Medium)",
            "",
            &rule,
            &format!("\u{1b}[38;5;111m>\u{1b}[39m {input_row}"),
            &rule,
            "\u{1b}[38;5;109m? for shortcuts\u{1b}[39m",
        ]
        .join("\n")
    }

    fn hint(text: &str) -> String {
        format!("\u{1b}[38;5;109m{text}\u{1b}[39m")
    }

    #[test]
    fn mode_hints_read_as_an_empty_input() {
        for text in [
            // The issue's capture.
            "Accept-edits mode: file edits auto-approved (shif...",
            // Live 1.4.9 at 120 columns.
            "Accept-edits mode: file edits auto-approved (shift+tab to cycle)",
            "Auto mode: edits & safe commands auto-approved (shift+tab to cycle)",
            "Plan mode: research & plan only (shift+tab to cycle)",
            "Best-of-N mode: fork into parallel arms and pick a winner (shift+tab to cycle)",
            // Live 1.4.9 at 57 columns.
            "Accept-edits mode: file edits auto-approved (shift+...",
            "Auto mode: edits & safe commands auto-approved (shi...",
            // A mode a newer Antigravity may add.
            "Review mode: every change waits for you (shift+tab to cycle)",
        ] {
            let screen = screen(&hint(text));
            assert!(
                !detect_session_chat_composer_ready(Some("antigravity"), &screen)
                    .blocks_message_for(Some("antigravity")),
                "{text}"
            );
            let input = session_chat_composer_input("antigravity", &screen).unwrap();
            assert!(input.is_empty(), "{text}");
        }
        // Default mode shows no hint.
        assert!(session_chat_composer_input("antigravity", &screen(""))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn typed_text_with_the_same_words_stays_a_draft() {
        for text in [
            "Accept-edits mode: file edits auto-approved (shift+tab to cycle)",
            "Accept-edits mode: file edits auto-approved (shif...",
            "Plan mode: research & plan only",
            "Plan mode: research & plan only (shift+tab to cycle) and then fix #349",
        ] {
            let input = session_chat_composer_input("antigravity", &screen(text)).unwrap();
            assert!(!input.is_empty(), "{text}");
            assert_eq!(input.text, text);
        }
        // Coloured, but not a hint Antigravity draws.
        for text in ["Plan the release", "Accept-edits mode: on (shift+tab)"] {
            assert!(
                !session_chat_composer_input("antigravity", &screen(&hint(text)))
                    .unwrap()
                    .is_empty(),
                "{text}"
            );
        }
    }

    /// The input rows of zmx 0.8.1's VT capture (`zmx history --vt --screen`, the capture
    /// gxserver reads) of a live Antigravity CLI 1.4.9, which serializes its colours as RGB.
    #[test]
    fn zmx_captures_tell_the_hint_from_typed_text() {
        let rule = "\u{1b}[0m\u{1b}[38;2;60;64;67m────────────────────────────────────────";
        let capture = |row: &str| {
            format!("{rule}\n\u{1b}[0m\u{1b}[38;2;138;180;248m>\u{1b}[1C\u{1b}[0m{row}\n{rule}\n\u{1b}[0m? for shortcuts")
        };
        let hint = capture("\u{1b}[38;2;154;160;166mAccept-edits mode: file edits auto-approved (shift+tab to cycle)\u{1b}[0m");
        assert!(session_chat_composer_input("antigravity", &hint)
            .unwrap()
            .is_empty());
        let typed =
            session_chat_composer_input("antigravity", &capture("Accept-edits mode: file edits"))
                .unwrap();
        assert!(!typed.is_empty());
        assert_eq!(typed.text, "Accept-edits mode: file edits");
    }

    #[test]
    fn a_message_pasted_over_the_hint_reads_as_the_message() {
        // Typing replaces the hint, so paste verification reads the message, not the hint.
        let input =
            session_chat_composer_input("antigravity", &screen("fix the failing test")).unwrap();
        assert!(!input.is_empty());
        assert_eq!(input.text, "fix the failing test");
        // A message that spells the hint itself still reads as typed.
        let typed = "Accept-edits mode: file edits auto-approved (shift+tab to cycle)";
        assert_eq!(
            session_chat_composer_input("antigravity", &screen(typed))
                .unwrap()
                .text,
            typed
        );
    }
}

#[cfg(test)]
mod claude_ide_pill_tests {
    use super::*;
    use crate::session_chat_composer::{
        claude_composer_input_text, detect_session_chat_composer_ready,
    };

    /// The bottom of a Claude Code 2.1.312 screen with `/ide` connected: the input row starts with
    /// the IDE pill in the theme's `ide` colour (RGB 71,130,200 in the dark theme), typed text in
    /// the default colour after it.
    fn screen(input_row: &str) -> String {
        let rule = "─".repeat(80);
        [
            "✻ Churned for 6s".to_string(),
            rule.clone(),
            format!("❯ {input_row}"),
            rule,
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)".to_string(),
        ]
        .join("\n")
    }

    fn pill(label: &str) -> String {
        format!("\u{1b}[38;2;71;130;200m[⧉ {label}]\u{1b}[39m ")
    }

    const LABELS: &[&str] = &[
        "In main.rs",
        "3 lines selected",
        "1 line selected",
        "12 lines from main.rs",
        "In session_chat_co…poser_input.rs",
    ];

    #[test]
    fn the_ide_pill_reads_as_an_empty_input() {
        for label in LABELS {
            let screen = screen(&pill(label));
            let input = session_chat_composer_input("claude", &screen).unwrap();
            assert!(input.is_empty(), "{label}: {:?}", input.text);
            assert!(
                !detect_session_chat_composer_ready(Some("claude"), &screen)
                    .blocks_message_for(Some("claude")),
                "{label}"
            );
            assert_eq!(
                claude_composer_draft(&screen).as_deref(),
                Some(""),
                "{label}"
            );
            assert_eq!(claude_composer_input_text(&screen), None, "{label}");
        }
        // ANSI themes colour it by palette index.
        let ansi = screen("\u{1b}[94m[⧉ In main.rs]\u{1b}[39m ");
        assert!(session_chat_composer_input("claude", &ansi)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn text_typed_after_the_pill_reads_as_the_draft() {
        for label in LABELS {
            let screen = screen(&format!("{}fix the failing test", pill(label)));
            let input = session_chat_composer_input("claude", &screen).unwrap();
            assert!(!input.is_empty(), "{label}");
            assert_eq!(input.text, "fix the failing test", "{label}");
            assert_eq!(
                claude_composer_draft(&screen).as_deref(),
                Some("fix the failing test"),
                "{label}"
            );
        }
    }

    #[test]
    fn typed_brackets_stay_a_draft() {
        // The same characters typed by hand carry the default colour.
        for typed in ["[⧉ In main.rs]", "[⧉ 3 lines selected] and more"] {
            let input = session_chat_composer_input("claude", &screen(typed)).unwrap();
            assert!(!input.is_empty(), "{typed}");
            assert_eq!(input.text, typed);
        }
        // Coloured, but not a label Claude's pill spells.
        let other = screen("\u{1b}[38;2;71;130;200m[⧉ Notes]\u{1b}[39m ");
        assert!(!session_chat_composer_input("claude", &other)
            .unwrap()
            .is_empty());
    }

    /// The input rows as zmx's VT capture (`zmx history --vt --screen`) serializes them.
    #[test]
    fn zmx_captures_tell_the_pill_from_typed_text() {
        let rule = "\u{1b}[0m\u{1b}[38;2;136;136;136m────────────────────────────────────────";
        let capture = |row: &str| {
            format!("{rule}\n\u{1b}[0m❯\u{1b}[1C{row}\n{rule}\n\u{1b}[0m  ? for shortcuts")
        };
        for label in ["In main.rs", "3 lines selected"] {
            let screen = capture(&format!(
                "\u{1b}[38;2;71;130;200m[⧉ {label}]\u{1b}[0m\u{1b}[7m \u{1b}[0m"
            ));
            assert!(
                session_chat_composer_input("claude", &screen)
                    .unwrap()
                    .is_empty(),
                "{label}"
            );
            assert!(
                !detect_session_chat_composer_ready(Some("claude"), &screen)
                    .blocks_message_for(Some("claude")),
                "{label}"
            );
        }
        let typed = session_chat_composer_input(
            "claude",
            &capture("\u{1b}[38;2;71;130;200m[⧉ In main.rs]\u{1b}[0m hello"),
        )
        .unwrap();
        assert_eq!(typed.text, "hello");
    }
}
