use std::ops::Range;

use super::{
    box_interior, strip_ansi_sgr, styled_lines, vt_capture_cursor, SessionChatComposerInput, Style,
    StyledLine,
};

/// Empryo's prompt glyph while no turn runs: `◈` focused, `◇` unfocused.
const EMPRYO_IDLE_MARKERS: &[char] = &['◈', '◇'];
/// The spinner frames Empryo draws in place of the prompt glyph while a turn runs.
const EMPRYO_BUSY_MARKERS: &[char] = &['◍', '◉', '◎'];
/// Rows Empryo paints below its input box: the statusline, with slack for a wrapped one.
const EMPRYO_FOOTER_MAX_LINES: usize = 3;

/// CDXC:AgentScreenDetection 2026-10-06 WHY:
/// Empryo 3.9.0-beta draws its input as the lowest rounded box on screen with only its statusline below, the first row opening with its prompt glyph (`◈`/`◇`, or a `◍◉◎` spinner frame while a turn runs; `$_n` and the input-box marker in its TUI bundle). Its question, approval, slash-command and queued-message panels are rounded boxes too, but they sit above the input box, and a question squeezes the input box to its top border, so the input exists only while that box is whole and last. Attached images are small boxes drawn inside it above the glyph row, so the input starts at that row.
pub(in crate::session_chat_composer) fn empryo_input_region(
    lines: &[String],
) -> Option<Range<usize>> {
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

/// CDXC:AgentScreenDetection 2026-10-06 WHY:
/// Empryo's empty input shows a rotating tip in its theme's muted color, and typed text differs from it only by color, which every theme sets differently. The cursor is the theme-free witness: it rests on the first input cell exactly when the input is empty. wmx's styled-row capture carries no cursor, so text there is marked unreadable: placing a Chat draft (which clears the box) and the attached-image clear stop with an error instead of wiping it, and carrying a terminal draft to Chat carries nothing and clears nothing, which keeps it in the terminal.
pub(super) fn empryo_composer_input(
    screen: &str,
    lines: &[StyledLine],
) -> Option<SessionChatComposerInput> {
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
