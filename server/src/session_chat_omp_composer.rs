use std::ops::Range;

use super::{is_horizontal_rule, is_titled_horizontal_rule, StyledLine};

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's box composer draws its statusline into the top border, and that line changes with the symbol preset (`π >` with Unicode symbols, `󰵗` and powerline glyphs with Nerd Font) and turns into a spinner mid-turn. Requiring `π` and `>` there read a Nerd Font OMP as never ready, so its first chat message waited in the queue forever.
/// The frame is the signature instead: OMP merges the input's last row into the foot (`╰─ text ─╯`), while its dialogs and welcome card close with a solid box rule.
pub(in crate::session_chat_composer) fn omp_input_region(lines: &[String]) -> Option<Range<usize>> {
    omp_input(lines).map(|(region, _)| region)
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

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// OMP's setup wizard asks every new user to pick one of eight composer shapes, and Ghostex read only the box and the band, so a user who picked any other one could never send from chat. Each shape is read by its own row grammar (measured against OMP 18.8.2's `packages/tui/src/components/composer/`), tried in an order where a looser grammar cannot claim a stricter shape's rows.
/// Every shape but the box and the band ends in OMP's status bar, and while the slash-command list is open the list replaces the status bar. So an input counts only when nothing but those follows it.
pub(super) fn omp_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
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

/// Rows OMP may draw under its input: the one-line status bar, or the slash-command list (two-space rows with one row selected by OMP's list cursor), which hides the status bar.
fn omp_composer_tail(rows: &[String]) -> bool {
    let rows: Vec<_> = rows.iter().filter(|row| !row.trim().is_empty()).collect();
    let status_bar = rows.len() <= 1
        && rows
            .iter()
            .all(|row| !row.trim_start().starts_with(['╭', '╰', '│', '❯']));
    status_bar || omp_slash_list(&rows)
}

/// OMP's slash-command list: two-space rows with one row selected by OMP's list cursor, drawn under every shape's input.
///
/// CDXC:AgentScreenDetection 2026-10-28 WHY:
/// OMP draws the list's selected row with its symbol preset's `nav.cursor` (`❯` with Unicode symbols, `\u{f054}` with Nerd Font, `>` with ASCII), while the composer's own gutter is a literal `❯` in every preset. Reading only `❯` here called a Nerd Font or ASCII OMP's input "not on screen" while `/switch …` showed its list, so a model change from chat typed the command and never pressed Enter.
fn omp_slash_list(rows: &[&String]) -> bool {
    !rows.is_empty()
        && rows
            .iter()
            .all(|row| omp_list_cursor_row(row) || row.starts_with("  "))
        && rows.iter().filter(|row| omp_list_cursor_row(row)).count() == 1
}

/// A list row OMP's cursor selects: `nav.cursor`, then a space.
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

/// `rule` and `borderless` shapes: a `❯ ` row then two-space rows, above the status bar or the slash list. The slash list draws two-space rows and a cursor row (`❯ ` with Unicode symbols), so the input is the topmost `❯ ` row of the trailing gutter block whose rows are followed only by that tail.
fn omp_gutter_input(lines: &[String]) -> Option<(Range<usize>, OmpShape)> {
    let gutter =
        |line: &str| omp_marked_row(line) || omp_list_cursor_row(line) || line.starts_with("  ");
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
