use std::ops::Range;

use super::{is_horizontal_rule, is_titled_horizontal_rule, strip_ansi_sgr};

/// CDXC:SessionChat 2026-09-08 WHY:
/// Rewind restores wrapped drafts in both Claude and Codex. Claude's former three-row signature rejected those drafts before Send could clear them.
/// Readiness, rewind and replacement must agree on the whole input region, including empty and continued rows.
pub(super) fn rule_input_region(lines: &[String], marker: char) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_titled_horizontal_rule(line))?;
    let start = (head + 1..foot).find(|&i| !lines[i].trim().is_empty())?;
    lines[start]
        .trim_start()
        .starts_with(marker)
        .then_some(start..foot)
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
    let lines: Vec<_> = screen.lines().map(strip_ansi_sgr).collect();
    let region = rule_input_region(&lines, '❯')?;
    let text = lines[region.start].trim_start().strip_prefix('❯')?;
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
    placeholder: bool,
}

impl SessionChatComposerInput {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() || self.placeholder
    }
}

#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    faint: bool,
    inverse: bool,
    foreground_rgb: Option<[u16; 3]>,
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
                                    7 => style.inverse = true,
                                    27 => style.inverse = false,
                                    30..=37 | 39 | 90..=97 => style.foreground_rgb = None,
                                    22 => {
                                        style.bold = false;
                                        style.faint = false;
                                    }
                                    38 | 48 | 58 => {
                                        if values[index] == 38 {
                                            style.foreground_rgb =
                                                if values.get(index + 1) == Some(&2) {
                                                    values
                                                        .get(index + 2..index + 5)
                                                        .and_then(|rgb| rgb.try_into().ok())
                                                } else {
                                                    None
                                                };
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

/// Input only, excluding transcript and footer. Call with a VT capture when proving a draft empty.
pub fn session_chat_composer_input(agent: &str, screen: &str) -> Option<SessionChatComposerInput> {
    if agent == "grok" {
        return super::grok_composer_draft(screen).map(|text| {
            // CDXC:AgentScreenDetection 2026-09-09 WHY: Grok's empty composer paints "Type a message..." in RGB 78,78,78 rather than SGR faint. Treating it as a draft held model selections forever; checking its VT style protects real input with the same words.
            let placeholder = text == "Type a message..."
                && styled_lines(screen)
                    .iter()
                    .rev()
                    .find(|line| super::is_boxed_marker_line(&line.text, '❯'))
                    .is_some_and(|line| {
                        let Some(marker) = line.chars.iter().position(|(ch, _)| *ch == '❯')
                        else {
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
                        line.chars[marker + 1..border]
                            .iter()
                            .filter(|(ch, _)| !ch.is_whitespace())
                            .all(|(_, style)| style.foreground_rgb == Some([78, 78, 78]))
                    });
            SessionChatComposerInput {
                text,
                rows: 1,
                placeholder,
            }
        });
    }
    let lines = styled_lines(screen);
    let plain: Vec<_> = lines.iter().map(|line| line.text.clone()).collect();
    let region = match agent {
        "claude" | "openclaude" => rule_input_region(&plain, '❯')?,
        "antigravity" => rule_input_region(&plain, '>')?,
        "cursor" => cursor_input_region(&plain)?,
        "codex" => {
            let start = lines.iter().rposition(|line| {
                line.chars
                    .iter()
                    .find(|(ch, _)| !ch.is_whitespace())
                    .is_some_and(|(ch, style)| {
                        matches!(ch, '›' | '»') && style.bold && !style.faint
                    })
            })?;
            let last = (start + 1..lines.len()).rfind(|&i| !plain[i].trim().is_empty())?;
            let foot = (start + 1..last).rfind(|&i| plain[i].trim().is_empty())?;
            start..foot
        }
        _ => return None,
    };
    let first = &lines[region.start];
    let marker = first.chars.iter().position(|(ch, _)| !ch.is_whitespace())?;
    let body: Vec<_> = first.chars[marker + 1..]
        .iter()
        .chain(
            lines[region.start + 1..region.end]
                .iter()
                .flat_map(|line| line.chars.iter()),
        )
        .filter(|(ch, _)| !ch.is_whitespace())
        .collect();
    let text = std::iter::once(
        first.chars[marker + 1..]
            .iter()
            .map(|(ch, _)| *ch)
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
    let placeholder = !body.is_empty()
        && body.iter().enumerate().all(|(index, (_, style))| {
            style.faint || (agent == "cursor" && index == 0 && style.inverse && body.len() > 1)
        })
        && !text.to_lowercase().contains("[paste");
    Some(SessionChatComposerInput {
        text,
        rows: region.len(),
        placeholder,
    })
}
