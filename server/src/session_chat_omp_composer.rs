use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use super::{is_horizontal_rule, is_titled_horizontal_rule, StyledLine};

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's box composer draws its statusline into the top border, and that line changes with the symbol preset (`π >` with Unicode symbols, `󰵗` and powerline glyphs with Nerd Font) and turns into a spinner mid-turn. Requiring `π` and `>` there read a Nerd Font OMP as never ready, so its first chat message waited in the queue forever.
/// The frame is the signature instead: OMP merges the input's last row into the foot (`╰─ text ─╯`), while its dialogs and welcome card close with a solid box rule.
pub(in crate::session_chat_composer) fn omp_input_region(lines: &[String]) -> Option<Range<usize>> {
    omp_input(lines).map(|(region, _)| region)
}

/// The first row OMP's live composer draws: the row of chrome above the input (the box's top border, the band's status row, the claude, pi and rule shapes' top rule), or the input's first row for the borderless, field and rail shapes, which draw nothing above it.
pub(in crate::session_chat_composer) fn omp_composer_top(lines: &[String]) -> Option<usize> {
    let (region, shape) = omp_input(lines)?;
    Some(omp_composer_span(lines, &region, shape).0)
}

/// The first and last rows the composer of `region` draws. A rule right above or below a gutter input, Unicode or ASCII, is the claude or rule shape's own.
fn omp_composer_span(lines: &[String], region: &Range<usize>, shape: OmpShape) -> (usize, usize) {
    let rule = |row: usize| {
        lines.get(row).is_some_and(|line| {
            is_horizontal_rule(line)
                || is_titled_horizontal_rule(line)
                || line.starts_with(OMP_ASCII_RULE)
        })
    };
    let top = match shape {
        OmpShape::Box | OmpShape::Band | OmpShape::MarkedSandwich | OmpShape::PaddedSandwich => {
            region.start - 1
        }
        OmpShape::Gutter => region
            .start
            .checked_sub(1)
            .filter(|&above| rule(above))
            .unwrap_or(region.start),
        OmpShape::Field | OmpShape::Rail => region.start,
    };
    let last = match shape {
        OmpShape::MarkedSandwich | OmpShape::PaddedSandwich => region.end,
        OmpShape::Gutter if rule(region.end) => region.end,
        _ => region.end - 1,
    };
    (top, last)
}

/// OMP's `chip.paste` symbol in its Unicode, Nerd Font and ASCII symbol presets.
const OMP_PASTE_CHIP_GLYPHS: &[&str] = &["\u{1f4c4}", "\u{f15c}", "txt"];

/// OMP's `nav.cursor` symbol, the selection marker of its lists, in its Unicode, Nerd Font and ASCII symbol presets.
pub(crate) const OMP_NAV_CURSOR_GLYPHS: &[char] = &['❯', '\u{f054}', '>'];

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
pub(super) enum OmpShape {
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

/// The input found by its shape's grammar, unless an overlay is drawn across it or the rows under it, or an ASCII frame sits on it.
///
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// OMP composites a selector or other overlay into the screen's rows at the overlay's column (`packages/tui/src/tui.ts`, `render/composite.ts`), so rows under a centred overlay keep the composer, status and widget text on their left and the overlay's rounded frame starts mid-row, where the leading-glyph checks and the blocking detector's frame scan do not look. Read as ready, the overlay's side bars became part of the draft and a chat message would have gone to the selector. The overlay is recognised by a border of its frame drawn after other text: a top border (`╭─` up to a `╮`) or a solid bottom border (`╰─…─╯`), as `chrome/overlay-box.ts` draws them. A lone corner glyph inside a draft or a widget line is text, not a frame.
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// With ASCII symbols those frames are drawn with `+ - |`, which Ghostex does not read as frames, so a live Ask mounted on a parked draft or an overlay composited over the composer read as a ready input once rows under it no longer held it. An ASCII frame border (`+-`, then the title if it is a top border, then a closing `+` right after a `-`) on the composer's first or last row, or on the row right above or below it, refuses the input. The title can hold a `+` of its own (`C++ target`), so the closing corner is the one a `-` leads into. Nowhere else is an ASCII frame read, so a `+---+` in a draft row between the composer's first and last rows stays text.
pub(super) fn omp_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    let (region, shape) = omp_shape_input(lines)?;
    let overlay = lines[region.start.saturating_sub(1)..]
        .iter()
        .any(|line| omp_embedded_frame_border(line));
    let (top, last) = omp_composer_span(lines, &region, shape);
    let ascii_frame = [top.checked_sub(1), Some(top), Some(last), Some(last + 1)]
        .into_iter()
        .flatten()
        .filter_map(|row| lines.get(row))
        .any(|line| omp_ascii_frame_border(line));
    (!overlay && !ascii_frame).then_some((region, shape))
}

/// Whether `line` holds a frame border as OMP's ASCII symbols draw it: `+-`, the title if it is a top border, and a closing `+` right after a `-`.
fn omp_ascii_frame_border(line: &str) -> bool {
    line.match_indices("+-").any(|(index, _)| {
        let border = &line[index + 1..];
        border
            .match_indices('+')
            .any(|(close, _)| border[..close].ends_with('-'))
    })
}

/// Whether `line` holds a rounded frame's top or solid bottom border after other text.
fn omp_embedded_frame_border(line: &str) -> bool {
    let row = line.trim();
    row.char_indices().skip(1).any(|(index, ch)| {
        let rest = &row[index + ch.len_utf8()..];
        match ch {
            '╭' => rest.starts_with('─') && rest.contains('╮'),
            '╰' => rest
                .split_once('╯')
                .is_some_and(|(rule, _)| !rule.is_empty() && rule.chars().all(|c| c == '─')),
            _ => false,
        }
    })
}

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// OMP's setup wizard asks every new user to pick one of eight composer shapes, and Ghostex read only the box and the band, so a user who picked any other one could never send from chat. Each shape is read by its own row grammar (measured against OMP 18.8.2's `packages/tui/src/components/composer/`), tried in an order where a looser grammar cannot claim a stricter shape's rows.
fn omp_shape_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
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

/// Glyphs that open a row of OMP chrome: each row of its frames (`theme.boxRound`'s corners, side and divider junction, which draw its dialogs and overlays), `╭ ╰ │ ├` with Unicode and Nerd Font symbols and `+ |` with ASCII symbols; and the chrome of its other composer shapes, the `─` rules of claude, pi and rule, the field and rail caps, and the `❯` gutter.
const OMP_CHROME_LEADS: &[char] = &['╭', '╰', '│', '├', '+', '|', '─', '▐', '▎', '❯'];

/// The claude, pi and rule shapes' rule with ASCII symbols, a full row of `-`; a single dash opens ordinary list text, so only a run counts.
///
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// With ASCII symbols those shapes draw their rules with `-`, which `OMP_CHROME_LEADS` leaves out, so a tool's output block left above them from before a preset change was read as a box input with their rows under it. Only the box tail refuses this rule: the borderless reader still finds an ASCII claude or rule input above its own rule.
const OMP_ASCII_RULE: &str = "---";

/// Rows OMP may draw under its input: the slash-command list right under it (two-space rows with one row selected by OMP's list cursor), then its status bar and extension statuses and widgets.
///
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// OMP (read from 18.13.3's `packages/coding-agent/src/modes/interactive-mode.ts` and `packages/tui/src/status-line/`) draws any number of rows under every shape's input: one row per extension status, extension widgets of up to ten lines that wrap on narrow panes, and the status bar under every shape but the box and the band. Counting them (none under the box, one under the other shapes) called an input with an extension status or widget under it "not on screen", so every chat message to it waited in the queue. Every shape now takes the band's rule through this one check: any rows, unless one opens with OMP chrome (a frame glyph, another shape's rule, cap or gutter), plus at most one list cursor; this replaces the one-row count.
/// The chrome check also keeps a box drawn above a different composer (a tool's output block with its exit code in the foot, an attachment card) from being read as the input. With Unicode symbols the list cursor and the borderless gutter are both `❯`, but OMP draws the list directly under the input's last row and keeps a gap row or widgets above every editor but the band's, so a `❯` row is the list's only in the run of list rows that starts under the input (after the blank rows of a band or gutter input's empty draft rows, see `omp_tail_start` and `omp_gutter_rows`).
fn omp_composer_tail(rows: &[String]) -> bool {
    let list = rows.iter().take_while(|row| omp_list_row(row)).count();
    rows.iter().filter(|row| omp_list_cursor_row(row)).count() <= 1
        && rows.iter().enumerate().all(|(index, row)| {
            (index < list && omp_list_cursor_row(row))
                || !row.trim_start().starts_with(OMP_CHROME_LEADS)
        })
}

/// A row of OMP's slash list: its cursor row or a two-space row.
fn omp_list_row(row: &str) -> bool {
    !row.trim().is_empty() && (omp_list_cursor_row(row) || row.starts_with("  "))
}

/// A list row OMP's cursor selects: `nav.cursor`, then a space.
///
/// CDXC:AgentScreenDetection 2026-10-28 WHY:
/// OMP draws the list's selected row with its symbol preset's `nav.cursor` (`❯` with Unicode symbols, `\u{f054}` with Nerd Font, `>` with ASCII), while the composer's own gutter is a literal `❯` in every preset. Reading only `❯` here called a Nerd Font or ASCII OMP's input "not on screen" while `/switch …` showed its list, so a model change from chat typed the command and never pressed Enter.
fn omp_list_cursor_row(row: &str) -> bool {
    row.strip_prefix(OMP_NAV_CURSOR_GLYPHS)
        .is_some_and(|rest| rest.starts_with(' '))
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

/// `claude` and `pi` shapes: the input sits between two full-width rules (the top one can carry a status chip), with the rows `omp_composer_tail` accepts under the foot. The foot is a plain rule: both shapes close with `─` across the row, while the rule shape's top rule carries the status chip. Both rules start at the row's first column, while OMP pads a reply's rows, its markdown rules included, by one cell, so a reply's rule above the rule shape's top rule is not taken for the head.
fn omp_sandwich_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    let foot = lines.iter().rposition(|line| omp_closing_rule(line))?;
    if !omp_composer_tail(&lines[foot + 1..]) {
        return None;
    }
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_horizontal_rule(line) || is_titled_horizontal_rule(line))?;
    if !lines[head].starts_with('─') {
        return None;
    }
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

/// `field` and `rail` shapes: every input row, blank ones included, starts with the shape's accent cap. OMP draws its gap row or extension widgets above the editor, so cap rows right under another shape's input, or under a `❯ ` gutter or list row, are a widget in that input's refused tail, not a field or rail input; two-space rows of a widget above the editor are not.
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
    let under_gutter = lines[..start]
        .iter()
        .rev()
        .take_while(|line| omp_list_row(line) || omp_marked_row(line))
        .any(|line| omp_marked_row(line) || omp_list_cursor_row(line));
    (!under_gutter && !omp_hangs_under_input(lines, start) && omp_composer_tail(&lines[end + 1..]))
        .then_some((start..end + 1, shape))
}

/// `rule` and `borderless` shapes: a `❯ ` row then two-space rows, above the rows `omp_composer_tail` accepts. Any `❯ ` row above the input has the input's own gutter under it, so the input is the topmost `❯ ` row followed only by that tail.
///
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// The slash list hangs its two-space rows and its cursor row (`❯ ` with Unicode symbols) directly under the input of every shape, while OMP keeps a gap row or extension widgets above every editor but the band's. A `❯ ` row in a list run that hangs under another shape's input (a box foot, a band, field or rail row, a sandwich's closing rule), or in the list under a refused `❯ ` row, is skipped; read as a borderless input, the list's selected command would pass for the draft.
fn omp_gutter_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    let mut from = 0;
    while let Some(row) = (from..lines.len()).find(|&row| omp_marked_row(&lines[row])) {
        if let Some((region, tail)) = omp_gutter_rows(lines, row) {
            if !omp_hangs_under_input(lines, row) && omp_composer_tail(&lines[tail..]) {
                return Some((region, OmpShape::Gutter));
            }
        }
        from = omp_rows(lines, row, |line| {
            line.starts_with("  ") || omp_list_cursor_row(line)
        })
        .end;
    }
    None
}

/// The rows of the gutter input whose `❯ ` row is `row`, and the row where the rows under it start; `None` while the screen cannot tell where its draft ends.
///
/// CDXC:AgentScreenDetection 2026-10-30 WHY:
/// OMP's slash list (`packages/tui/src/components/select-list.ts`) draws its rows before the selected one as two-space rows, like a draft's continuation rows, and an empty draft row as a blank row. Reading every two-space row as the draft took list items into it, and reading rows that line up with the selected row's description column as list items dropped draft rows that happened to line up: OMP pads its editor rows to the pane's width and leaves list rows unpadded, but the capture trims trailing spaces, so the screen cannot tell the two apart. While such a row stands right above the selected item the input is held, until the list closes or the row stops lining up; moving the selection to the top releases it only when that row was a list item. A two-space row that does not line up is the draft's, and a list drawn without descriptions has no column to line up with.
fn omp_gutter_rows(lines: &[String], row: usize) -> Option<(Range<usize>, usize)> {
    let run = omp_rows(lines, row, |line| {
        line.starts_with("  ") || omp_list_cursor_row(line)
    });
    let Some(cursor) = (row + 1..run.end).find(|&below| omp_list_cursor_row(&lines[below])) else {
        let region = omp_rows(lines, row, |line| line.starts_with("  "));
        let tail = region.end;
        return Some((region, tail));
    };
    let column = omp_list_columns(&lines[cursor]).1;
    let aligned_above =
        cursor - 1 > row && column.is_some() && omp_list_row(&lines[cursor - 1]) && {
            let (text, description) = omp_list_columns(&lines[cursor - 1]);
            description.or(text.filter(|&text| text >= OMP_LIST_MIN_DESCRIPTION_COLUMN)) == column
        };
    if aligned_above {
        return None;
    }
    let end = (row..cursor)
        .rfind(|&above| !lines[above].trim().is_empty())
        .unwrap_or(row);
    Some((row..end + 1, cursor))
}

/// The first cell a list row's description can start in: OMP gives a completion list's label column at least 12 cells (`minPrimaryColumnWidth` in `packages/tui/src/components/editor.ts`, 32 cells for its other lists), after the two-cell cursor or indent and any icon cell.
const OMP_LIST_MIN_DESCRIPTION_COLUMN: usize = 14;

/// The terminal columns where a list row's text and its description start, in cells as OMP's `visibleWidth` and the terminal count them: each column is the display width of the row before it, so an emoji sequence counts as the two cells it is drawn in.
///
/// OMP's select list (`packages/tui/src/components/select-list.ts`) lays a row out as the two-cell cursor or indent, an optional icon cell padded to the widest icon, the label with its whitespace collapsed to single spaces, and, for a described row, padding of at least two cells up to the description at `OMP_LIST_MIN_DESCRIPTION_COLUMN` or later. Padding that ends before that cell follows an icon, not a label, so a list drawn without descriptions has no description column even when its icons are padded. A wrapped description's later rows are indented to the description column, so their text start stands for it.
/// This holds for icons up to 10 cells wide; OMP's own preset icons are 1 or 2 cells. With an extension icon of 11 cells or more, the icon's padding can end at cell 14 or later and is taken for the description column. A list without descriptions then holds the message until the list closes, and a described list can read unselected rows into the draft.
fn omp_list_columns(row: &str) -> (Option<usize>, Option<usize>) {
    let Some((body, _)) = row.char_indices().nth(2) else {
        return (None, None);
    };
    let mut text = None;
    let mut spaces = 0;
    for (offset, ch) in row[body..].char_indices() {
        if ch == ' ' {
            spaces += 1;
            continue;
        }
        if text.is_none() {
            text = Some(row[..body + offset].width());
        } else if spaces >= 2 {
            let column = row[..body + offset].width();
            if column >= OMP_LIST_MIN_DESCRIPTION_COLUMN {
                return (text, Some(column));
            }
        }
        spaces = 0;
    }
    (text, None)
}

/// Where the rows under a band input start: OMP draws an empty draft row as a blank row, so blank rows between the input's last text and the slash list still belong to the input.
fn omp_tail_start(lines: &[String], end: usize) -> usize {
    let below = (end..lines.len())
        .find(|&row| !lines[row].trim().is_empty())
        .unwrap_or(lines.len());
    let list = lines[below..]
        .iter()
        .take_while(|line| omp_list_row(line))
        .any(|line| omp_list_cursor_row(line));
    if list {
        below
    } else {
        end
    }
}

/// Whether the list run holding `row` starts under the last row of a box, band, field, rail or sandwich input: directly, or, for the band, after the blank rows of its empty draft lines.
fn omp_hangs_under_input(lines: &[String], row: usize) -> bool {
    let top = lines[..row]
        .iter()
        .rposition(|line| !omp_list_row(line))
        .map_or(0, |above| above + 1);
    let Some(above) = top.checked_sub(1) else {
        return false;
    };
    let line = lines[above].trim();
    let band_end = lines[..top]
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map(|last| last + 1);
    (line.starts_with('╰') && line.ends_with('╯'))
        || line.starts_with(['▐', '▎'])
        || band_end.is_some_and(|end| {
            omp_band_input_region(&lines[..end]).is_some_and(|region| region.end == end)
        })
        || (omp_closing_rule(line)
            && omp_sandwich_input(&lines[..top]).is_some_and(|(region, _)| region.end == above))
}

/// The plain rule that closes the claude and pi shapes: `─` across the row, with no status chip.
fn omp_closing_rule(line: &str) -> bool {
    is_horizontal_rule(line) && line.trim().chars().all(|ch| ch == '─')
}

/// One input row's text with the shape's chrome removed.
pub(super) fn omp_row_text(shape: OmpShape, first: bool, line: &str) -> String {
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
    let tail = &lines[foot + 1..];
    if !omp_composer_tail(tail)
        || tail
            .iter()
            .any(|row| row.trim_start().starts_with(OMP_ASCII_RULE))
    {
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
    omp_composer_tail(&lines[omp_tail_start(lines, end + 1)..]).then_some(cue..end + 1)
}

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's empty composer shows one right-aligned gesture hint in its foot (`╰─  ⇧⇥ to change thinking effort ─╯`, or `󰘶 󰌒 …` with Nerd Font symbols): the key as one accent-colored span, the label italic. Reading it as a draft failed every send with "could not be cleared". Typed input is never italic, so an italic label with only one key span before it is the hint.
pub(super) fn omp_hint_only(line: &StyledLine) -> bool {
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
