//! CDXC:AgentScreenDetection 2026-10-28 WHY:
//! dsh-tui 0.17 draws its input as the lowest rounded box on screen, with `❯` opening its first row. On the launchpad (the first screen of a new session) the row sits between `│` borders and holds a dim placeholder; on the chat page the row opens with the session-manager glyph `⌸` and ends with the full-screen editor glyph `⛶`, measured 2026-10-28:
//! `│ ❯ Say something, or type / for commands… │` and `⌸ ❯ ⛶`.
//! SEE-ALSO: server/src/session_chat_composer.rs (`ComposerSignature::DshInputBox`), server/src/session_chat_dsh_picker.rs (its status line).

use std::ops::Range;

use super::SessionChatComposerInput;

/// The launchpad's placeholder in dsh-tui's two languages.
const DSH_PLACEHOLDERS: &[&str] = &[
    "Say something, or type / for commands…",
    "说点什么，或输入 / 看命令…",
];

/// The tallest input box accepted: a few wrapped rows of a draft.
const DSH_MAX_INPUT_ROWS: usize = 12;

fn is_border_row(line: &str) -> bool {
    line.trim_start().starts_with(['\u{2502}', '\u{2338}'])
}

/// The rows of dsh-tui's input box: below the lowest `╭…╮` whose first row opens with `❯` (after
/// its `│` or `⌸` border) and above its `╰…╯`.
pub(crate) fn dsh_input_region(lines: &[String]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('\u{2570}') && line.ends_with('\u{256f}')
    })?;
    let head = lines[..foot].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('\u{256d}') && line.ends_with('\u{256e}')
    })?;
    let rows = head + 1..foot;
    if rows.is_empty() || rows.len() > DSH_MAX_INPUT_ROWS {
        return None;
    }
    let first = lines[rows.start].trim_start();
    let opens_with_prompt = is_border_row(first)
        && first
            .chars()
            .skip(1)
            .find(|ch| !ch.is_whitespace())
            .is_some_and(|ch| ch == '\u{276f}');
    let inner_rows_plain = lines[rows.clone()]
        .iter()
        .all(|line| !line.contains(['\u{256d}', '\u{2570}']));
    (opens_with_prompt && inner_rows_plain).then_some(rows)
}

fn row_text(line: &str) -> &str {
    let line = line.trim();
    let line = line
        .strip_prefix(['\u{2502}', '\u{2338}'])
        .unwrap_or(line)
        .trim_end();
    line.strip_suffix(['\u{2502}', '\u{26f6}'])
        .unwrap_or(line)
        .trim()
}

pub(crate) fn dsh_composer_input(lines: &[String]) -> Option<SessionChatComposerInput> {
    let region = dsh_input_region(lines)?;
    let mut rows: Vec<String> = lines[region.clone()]
        .iter()
        .map(|line| row_text(line).to_string())
        .collect();
    if let Some(first) = rows.first_mut() {
        *first = first
            .strip_prefix('\u{276f}')
            .unwrap_or(first)
            .trim_start()
            .to_string();
    }
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    let placeholder = rows.len() == 1 && DSH_PLACEHOLDERS.contains(&rows[0].as_str());
    Some(SessionChatComposerInput {
        text: if placeholder {
            String::new()
        } else {
            rows.join("\n")
        },
        rows: region.len(),
        shell_mode: false,
        placeholder,
        attachments: 0,
        text_unreadable: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(screen: &str) -> Vec<String> {
        screen.lines().map(str::to_string).collect()
    }

    const LAUNCHPAD: &str = "                                                  ██  ██  ██▄▄▄▄\n   ╭──────────────────────────────────────────╮\n   │ ❯ Say something, or type / for commands… │\n   ╰──────────────────────────────────────────╯\n     deepseek-flash  ·  Max  ·  standard  ·  Workspace read/write\n         Sessions & workspaces    Settings    Kernel · DSH    Help\n";

    const CHAT: &str = "  ⏺ DONE\n  ╭──────────────────────────────────────────────╮\n  ⌸ ❯                                            ⛶\n  ╰──────────────────────────────────────────────╯\n                                     9.5k/1.0M 1.0%\n   deepseek-flash · max · cache 98.3% · ≈¥0.02 peak · dsh-throwaway\n";

    #[test]
    fn the_launchpad_box_is_an_empty_input_with_its_placeholder() {
        let input = dsh_composer_input(&lines(LAUNCHPAD)).unwrap();
        assert!(input.placeholder);
        assert!(input.text.is_empty());
        assert_eq!(input.rows, 1);
    }

    #[test]
    fn the_chat_page_box_reads_empty_then_holds_a_draft() {
        let input = dsh_composer_input(&lines(CHAT)).unwrap();
        assert!(!input.placeholder);
        assert!(input.text.is_empty());
        let drafted = CHAT.replace("⌸ ❯     ", "⌸ ❯ fix the tests");
        assert_eq!(
            dsh_composer_input(&lines(&drafted)).unwrap().text,
            "fix the tests"
        );
    }

    #[test]
    fn a_box_without_the_prompt_is_not_the_input() {
        let dialog = "  ╭──────────╮\n  │ Allow?   │\n  ╰──────────╯\n";
        assert!(dsh_input_region(&lines(dialog)).is_none());
    }
}
