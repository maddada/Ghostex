//! What a VT screen capture says about styling that the plain capture drops.

use crate::session_chat_options::strip_ansi_sgr;

/// The plain capture a VT capture stands for: the same rows with every escape removed.
pub(crate) fn plain_screen_text(styled: &str) -> String {
    styled
        .split('\n')
        .map(strip_ansi_sgr)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads bold off a VT capture one row at a time. The pen carries over from row to row, so every
/// row, blank ones included, goes through the same scanner in order.
#[derive(Default)]
pub(crate) struct BoldRows {
    bold: bool,
}

impl BoldRows {
    /// Whether every visible character of `styled_line` in the char columns `start..end` is bold.
    /// Spaces and the `⏺` message bullet, which Claude paints in its own colour, do not count. A
    /// range with nothing visible in it is not bold.
    pub(crate) fn next_row(&mut self, styled_line: &str, start: usize, end: usize) -> bool {
        let mut column = 0;
        let mut all_bold = true;
        let mut seen = false;
        let mut chars = styled_line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                match chars.next() {
                    Some('[') => {
                        let mut params = String::new();
                        for inner in chars.by_ref() {
                            if ('@'..='~').contains(&inner) {
                                if inner == 'm' {
                                    self.bold = sgr_bold(&params, self.bold);
                                }
                                break;
                            }
                            params.push(inner);
                        }
                    }
                    Some(']') => {
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
                    Some('(' | ')' | '*' | '+') => {
                        chars.next();
                    }
                    _ => {}
                }
                continue;
            }
            if ch == '\r' {
                continue;
            }
            if (start..end).contains(&column) && !ch.is_whitespace() && ch != '⏺' {
                all_bold &= self.bold;
                seen = true;
            }
            column += 1;
        }
        seen && all_bold
    }
}

/// The bold state after one SGR sequence. Colour parameters are skipped whole, because a `1` inside
/// `38;2;1;…` is a colour component, not bold.
fn sgr_bold(params: &str, mut bold: bool) -> bool {
    // `ESC [ > 4 ; 2 m` and its kin set keyboard modes, not the pen.
    if params.starts_with(['<', '=', '>', '?']) {
        return bold;
    }
    let mut parts = params.split(';');
    while let Some(part) = parts.next() {
        match part {
            "" | "0" | "22" => bold = false,
            "1" => bold = true,
            "38" | "48" | "58" => match parts.next() {
                Some("5") => {
                    parts.next();
                }
                Some("2") => {
                    parts.next();
                    parts.next();
                    parts.next();
                }
                _ => {}
            },
            _ => {}
        }
    }
    bold
}
