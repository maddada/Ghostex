//! What a notice's "Show terminal output" opens to: the terminal lines above the prompt the card
//! already shows.
//!
//! CDXC:SessionChat 2026-10-27 DECISION: User answered Q682 "lines above the prompt": the open
//! terminal output on a card that answers a prompt shows only what the agent printed above it,
//! never the prompt again with its rows and key hints, which the card already draws. The prompt
//! starts at the bottom-most line naming its title (or, without one, its first row), widened up
//! over the box or rule the agent draws around it, and the newest 12 lines above it are shown. A
//! tail where the prompt cannot be found shows its newest 12 lines; one with nothing above the
//! prompt has no terminal output to show.
//! SEE-ALSO: `terminal_dialog_presentation` in terminal_prompts.rs (CDXC:SessionChat 2026-10-27),
//! apps/desktop/src/app/native_chat/notice.rs (the disclosure).

use crate::questions::model::TerminalNotice;

/// A line the agent draws as the frame of its prompt: box edges, rules and their corners.
fn is_frame(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with(['│', '┃', '╭', '┌', '╰', '└'])
        || (!trimmed.is_empty()
            && trimmed.chars().all(|character| {
                matches!(
                    character,
                    '─' | '━'
                        | '═'
                        | '▔'
                        | '▁'
                        | '╭'
                        | '╮'
                        | '╰'
                        | '╯'
                        | '┌'
                        | '┐'
                        | '└'
                        | '┘'
                        | ' '
                )
            }))
}

/// The words a prompt's first line can be found by, most specific first: the header a permission
/// prompt opens with ("Bash command"), the dialog's title, its first row.
fn markers(notice: &TerminalNotice, header: Option<&str>) -> Vec<String> {
    let mut markers: Vec<String> = header.map(str::to_string).into_iter().collect();
    if let Some(dialog) = &notice.dialog {
        markers.push(dialog.title.trim().to_string());
        if let Some(row) = dialog.rows.first() {
            markers.push(row.label.trim().to_string());
        }
    }
    markers.push(notice.title.trim().to_string());
    if let Some(choice) = notice.visible_choices().first() {
        markers.push(choice.label.trim().to_string());
    }
    markers.retain(|marker| marker.chars().count() >= 3);
    markers
}

/// How many lines the open terminal output shows, the newest ones: gxserver sends a longer tail so
/// that a prompt filling the screen still leaves lines above it.
const SHOWN_LINES: usize = 12;

/// The newest `SHOWN_LINES` of `lines`, joined.
fn newest(lines: &[&str]) -> Option<String> {
    let start = lines.len().saturating_sub(SHOWN_LINES);
    let shown = &lines[start..];
    let shown = match shown.iter().position(|line| !line.trim().is_empty()) {
        Some(first) => &shown[first..],
        None => return None,
    };
    Some(shown.join("\n"))
}

/// `tail` cut to the lines above the prompt `notice` answers, or `None` when nothing is above it.
/// A notice that answers no prompt keeps the end of its tail.
pub fn above_the_prompt(
    tail: &str,
    notice: &TerminalNotice,
    header: Option<&str>,
) -> Option<String> {
    let lines: Vec<&str> = tail.lines().collect();
    let answers_prompt = notice.dialog.is_some() || !notice.visible_choices().is_empty();
    if !answers_prompt {
        return newest(&lines);
    }
    let start = markers(notice, header).iter().find_map(|marker| {
        lines
            .iter()
            .rposition(|line| line.contains(marker.as_str()))
    });
    let Some(mut start) = start else {
        return newest(&lines);
    };
    while start > 0 && is_frame(lines[start - 1]) {
        start -= 1;
    }
    let mut above = &lines[..start];
    while let Some((last, rest)) = above.split_last() {
        if last.trim().is_empty() || is_frame(last) {
            above = rest;
        } else {
            break;
        }
    }
    newest(above)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn notice(value: serde_json::Value) -> TerminalNotice {
        TerminalNotice::parse(Some(&value)).expect("notice")
    }

    #[test]
    fn a_codex_approval_keeps_only_what_codex_printed_above_it() {
        // Verbatim from Codex 0.164 in Read Only (2026-10-27).
        let tail = "› Run exactly this one shell command and nothing else, then reply done: touch proof-readonly.txt\n• Running touch proof-readonly.txt\n\n  Would you like to run the following command?\n  Environment: local\n\n  $ touch proof-readonly.txt\n› 1. Yes, proceed (y)\n  2. Yes, and don't ask again for commands that start with `touch proof-readonly.txt` (p)\n  3. No, and tell Codex what to do differently (esc)\n  Press enter to confirm or esc to cancel";
        let codex = notice(json!({
            "kind": "approvalPrompt", "severity": "info", "title": "Would you like to run the following command?",
            "source": "terminal", "detectedAt": "2026-10-27T10:00:00.000Z",
            "choices": [{"index": 0, "label": "Yes, proceed (y)", "selected": true}],
            "dialog": {"id": "d", "title": "Would you like to run the following command?", "body": "", "footer": "",
                "rows": [{"number": 1, "label": "Yes, proceed (y)", "selected": true}], "actions": []},
        }));
        assert_eq!(
            above_the_prompt(tail, &codex, None).as_deref(),
            Some("› Run exactly this one shell command and nothing else, then reply done: touch proof-readonly.txt\n• Running touch proof-readonly.txt")
        );
    }

    #[test]
    fn a_boxed_prompt_drops_its_whole_box_and_a_prompt_alone_has_nothing_above() {
        let boxed = notice(json!({
            "kind": "permissionPrompt", "severity": "info", "title": "Do you want to proceed?",
            "source": "terminal", "detectedAt": "2026-10-27T10:00:00.000Z",
            "choices": [{"index": 0, "label": "Yes", "selected": true}, {"index": 1, "label": "No (esc)", "selected": false}],
        }));
        let tail = "⏺ Bash(touch x)\n╭──────────────────╮\n│ Bash command     │\n│ Do you want to proceed? │\n│ ❯ 1. Yes         │\n╰──────────────────╯";
        // "Bash command" is part of the box, so only what sits above the box stays.
        assert_eq!(
            above_the_prompt(tail, &boxed, None).as_deref(),
            Some("⏺ Bash(touch x)")
        );
        let alone = "Do you want to proceed?\n❯ 1. Yes\n  2. No (esc)";
        assert_eq!(above_the_prompt(alone, &boxed, None), None);
        // A notice that answers nothing keeps its tail.
        // Claude Code's input-slot prompt, verbatim (2.1, 2026-10-27): it opens with its tool header.
        let claude = notice(json!({
            "kind": "permissionPrompt", "severity": "info", "title": "Read the global agent instructions file",
            "source": "terminal", "detectedAt": "2026-10-27T10:00:00.000Z",
            "choices": [{"index": 0, "label": "Yes", "selected": true}, {"index": 3, "label": "No", "selected": false}],
        }));
        let claude_tail = " Reading the global agent instructions file\n ⎿  $ cat ~/.agents/main.md\n\nBash command\nTip: auto mode handles these prompts for you — choose \"switch to auto mode\" below\nRead the global agent instructions file\n\ncat ~/.agents/main.md\n\nDo you want to proceed?\n❯ 1. Yes\n  2. Yes, allow reading from /Users/madda/.agents from this project\n  4. No\nEsc to cancel · Tab to amend";
        assert_eq!(
            above_the_prompt(claude_tail, &claude, Some("Bash command")).as_deref(),
            Some(" Reading the global agent instructions file\n ⎿  $ cat ~/.agents/main.md")
        );
        let plain = notice(json!({
            "kind": "usageLimit", "severity": "warning", "title": "Usage limit reached",
            "source": "terminal", "detectedAt": "2026-10-27T10:00:00.000Z",
        }));
        assert_eq!(
            above_the_prompt("a\nb", &plain, None).as_deref(),
            Some("a\nb")
        );
        // The card shows the newest 12 lines of the longer tail gxserver sends.
        let long: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
        let shown = above_the_prompt(&long.join("\n"), &plain, None).unwrap();
        assert_eq!(shown.lines().count(), 12);
        assert!(shown.starts_with("line 19") && shown.ends_with("line 30"));
    }
}

#[cfg(test)]
mod numbers_only_tests {
    use crate::questions::model::TerminalDialog;
    use crate::questions::notice_choices::without_key_hint;
    use crate::questions::terminal_prompts::terminal_dialog_presentation;
    use serde_json::json;

    #[test]
    fn the_agents_own_key_hints_leave_the_row_labels() {
        assert_eq!(without_key_hint("Yes, proceed (y)"), "Yes, proceed");
        assert_eq!(
            without_key_hint("No, and tell Codex what to do differently (esc)"),
            "No, and tell Codex what to do differently"
        );
        assert_eq!(
            without_key_hint("Yes, and don't ask again for commands that start with `touch x` (p)"),
            "Yes, and don't ask again for commands that start with `touch x`"
        );
        // A bracket that names something other than a key stays.
        assert_eq!(
            without_key_hint("Install for you (user scope)"),
            "Install for you (user scope)"
        );
        assert_eq!(without_key_hint("Yes"), "Yes");
    }

    #[test]
    fn numbered_rows_are_the_buttons() {
        let dialog: TerminalDialog = serde_json::from_value(json!({
            "id": "d", "title": "Would you like to run the following command?", "body": "", "footer": "",
            "rows": [{"number": 1, "label": "Yes, proceed (y)", "selected": true},
                     {"number": 2, "label": "No, and tell Codex what to do differently (esc)", "selected": false}],
            "actions": ["up", "down", "confirm", "cancel"],
        }))
        .unwrap();
        assert_eq!(terminal_dialog_presentation(&dialog)["actions"], json!([]));
        // A panel whose choice is not a row keeps its Confirm and Cancel.
        let panel: TerminalDialog = serde_json::from_value(json!({
            "id": "p", "title": "Usage", "body": "❯ Keep going", "footer": "", "rows": [],
            "actions": ["confirm", "cancel"],
        }))
        .unwrap();
        let labels: Vec<String> = terminal_dialog_presentation(&panel)["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|action| action["action"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(labels, ["confirm", "cancel"]);
    }
}
