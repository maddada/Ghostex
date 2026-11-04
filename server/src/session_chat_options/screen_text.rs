use super::*;

// ---------------------------------------------------------------------------
// Line/segment preparation
// ---------------------------------------------------------------------------

/// A Nerd Font icon glyph (the private-use planes), which agents such as Empryo draw before labels.
pub(crate) fn is_nerd_font_icon(ch: char) -> bool {
    matches!(ch as u32, 0xE000..=0xF8FF | 0xF0000..=0x10FFFF)
}

/// Removes terminal control sequences while preserving visible text.
/// CDXC:AgentScreenDetection 2026-09-18 WHY:
/// Codex chat notices use VT captures, but dialog answers recheck plain captures.
/// Leaving OSC working-directory or hyperlink metadata in the text changes the dialog ID and rejects unchanged choices, including every update-prompt button.
/// CDXC:AgentScreenDetection 2026-10-26 WHY:
/// zmx's VT capture writes a run of never-painted cells as cursor-forward (`ESC[3C`), and Claude's Ink renderer leaves the gaps between words unpainted when it repaints, so dropping that escape deleted the spaces: "Ready to code?" read "Yes,manuallyapproveedits", only its highlighted row was a choice, and the card's dialog id never matched the plain capture the answer rechecks, refusing every pick as "Claude's dialog changed" (GitHub #323). A cursor-forward stands for that many blank cells.
pub(crate) fn strip_ansi_sgr(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                let mut params = String::new();
                for inner in chars.by_ref() {
                    if ('@'..='~').contains(&inner) {
                        if inner == 'C' {
                            out.extend(std::iter::repeat_n(' ', cursor_forward_cells(&params)));
                        }
                        break;
                    }
                    params.push(inner);
                }
            }
            Some(']') => {
                chars.next();
                while let Some(inner) = chars.next() {
                    if inner == '\u{7}' {
                        break;
                    }
                    if inner == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // A character-set designation (`ESC ( B`), which the VT capture appends after a
            // row that switched sets.
            Some('(' | ')' | '*' | '+') => {
                chars.next();
                chars.next();
            }
            _ => {}
        }
    }
    out
}

/// How many cells a cursor-forward (`ESC [ n C`) skips: its count, 1 when the count is left out.
pub(crate) fn cursor_forward_cells(params: &str) -> usize {
    if params.is_empty() {
        return 1;
    }
    // Capped so a hostile count cannot allocate without bound; a row is never this wide.
    params
        .parse::<usize>()
        .map_or(0, |cells| cells.clamp(1, 4096))
}

/// Claude renders its thread title inside a long `─` rule, and Codex renders
/// `─ Worked for … ─`. Skipping those lines keeps titles out of the scan.
pub(super) fn is_divider_line(line: &str) -> bool {
    let mut run = 0usize;
    for ch in line.chars() {
        if ch == '\u{2500}' {
            run += 1;
            if run >= 8 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/*
Claude Code renders its statusline with NON-BREAKING spaces (U+00A0), verified
on a live session: the segment arrives as `Fable\u{a0}5`. Folding every
whitespace character to a plain space is what makes the grammar match what the
user actually sees, instead of silently detecting only the space-free segments.
*/
pub(crate) fn normalize_spaces(line: &str) -> String {
    line.chars()
        .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
        .collect()
}

/// The last `SESSION_CHAT_OPTION_SCAN_LINES` non-blank lines, oldest first.
pub(super) fn scan_window(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw))
            .trim_end()
            .to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= SESSION_CHAT_OPTION_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

/// Trimmed segments of one statusline, split on `|` and `·`.
pub(super) fn line_segments(line: &str) -> Vec<String> {
    line.split(['|', '\u{00b7}'])
        .map(|segment| segment.trim().to_string())
        .collect()
}
