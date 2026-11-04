//! Claude Code's own updater notices, which it paints at the right of its screen near the input box:
//! "Update available! Run: mise upgrade claude" (and its npm, brew and `claude update` forms),
//! "✗ Auto-update failed · Try claude doctor or npm i -g @anthropic-ai/claude-code",
//! "current: 2.1.220 · latest: 2.1.220 ✘ Auto-update failed · Run claude doctor", "Auto-updating…" and
//! "✓ Update installed · Restart to apply". They are terminal chrome about the CLI itself, never
//! the session's status, so nothing the chat reads off the screen keeps them.
//!
//! CDXC:SessionChat 2026-10-29 DECISION:
//! User: "just now in the chat view i saw this text appear bottom right for your session 'Update available! Run: mise upgrade claude'. this is because it's shown in the terminal view, but we shouldn't show this". It came through Claude's footer capture (`terminal_status_line`, the context meter's tooltip at the composer's bottom right), which keeps every footer row down to the screen's end; the hint sat there. Both the footer capture (session_chat_options/detection.rs) and the screen-activity reader (session_chat_terminal_activity/screen_rows.rs) strip these notices, the whole row or the right-aligned part of a row, and leave everything else as it was.

/// Where an updater notice starts in a row, the first of these that is the row's start or follows a
/// gap of two spaces or more (a right-aligned notice beside other footer text).
const NOTICE_STARTS: &[&str] = &[
    "Update available! Run:",
    "current: ",
    "✗ Auto-update failed",
    "✘ Auto-update failed",
    "Auto-update failed",
    "Auto-updating",
    "✓ Update installed",
    "Update installed",
];

/// The row without a Claude Code updater notice: the part before it, trimmed at its end, or "" when
/// the whole row was the notice. A row without one comes back unchanged.
pub(crate) fn without_claude_update_notice(row: &str) -> &str {
    let trimmed_start = row.len() - row.trim_start().len();
    let mut cut: Option<usize> = None;
    for start in NOTICE_STARTS {
        for (at, _) in row.match_indices(start) {
            let before = &row[..at];
            let leads = at == trimmed_start;
            let right_aligned = before.ends_with("  ");
            if !(leads || right_aligned) || !is_notice_at(&row[at..]) {
                continue;
            }
            cut = Some(cut.map_or(at, |cut| cut.min(at)));
            break;
        }
    }
    match cut {
        Some(at) => row[..at].trim_end(),
        None => row,
    }
}

/// Whether `rest` (a row from a candidate start) really is an updater notice: "current: …" only
/// when it is the version pair the failed auto-update prints.
fn is_notice_at(rest: &str) -> bool {
    if rest.starts_with("current: ") {
        return rest.contains("latest: ") && rest.contains("Auto-update failed");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::without_claude_update_notice as strip;

    #[test]
    fn every_installer_form_of_the_hint_is_dropped() {
        for hint in [
            "Update available! Run: mise upgrade claude",
            "Update available! Run: npm i -g @anthropic-ai/claude-code",
            "Update available! Run: brew upgrade claude-code",
            "Update available! Run: claude update",
            "✗ Auto-update failed · Try claude doctor or npm i -g @anthropic-ai/claude-code",
            "current: 2.1.220 · latest: 2.1.220 ✘ Auto-update failed · Run claude doctor",
            "Auto-updating…",
            "✓ Update installed · Restart to apply",
        ] {
            let painted = format!("{}{hint}", " ".repeat(150));
            assert_eq!(strip(&painted), "", "{hint}");
            assert_eq!(strip(hint), "", "{hint}");
        }
    }

    #[test]
    fn a_right_aligned_hint_leaves_the_footer_text_beside_it() {
        assert_eq!(
            strip(
                "  ⏵⏵ bypass permissions on (shift+tab to cycle)                Update available! Run: mise upgrade claude"
            ),
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)"
        );
    }

    #[test]
    fn real_status_text_is_kept() {
        for row in [
            "  Opus 5.5 | xhigh | Ctx 69% | $919.03 | 5h 7% · 7d 58%",
            "⏺ The update notice stays out of the chat: \"Update available! Run: mise upgrade claude\" is Claude Code's own hint",
            "  current: the second row of the table",
            "✻ Churned for 25s · done 2:59 AM",
            "Update the README with the new flags",
        ] {
            assert_eq!(strip(row), row, "{row}");
        }
    }

    /// A Claude screen with the hint painted at the bottom right, under its footer, and once more
    /// right above the input box: neither the footer the context meter shows nor the streamed
    /// message the chat mirrors carries it.
    #[test]
    fn a_frame_with_the_hint_gives_the_chat_no_such_text() {
        let rule = "─".repeat(60);
        for width in [200usize, 60] {
            let hint = "Update available! Run: mise upgrade claude";
            let right = |text: &str| {
                format!(
                    "{}{text}",
                    " ".repeat(width.saturating_sub(text.chars().count() + 2))
                )
            };
            let screen = [
                "⏺ I sent both to the thread that built that bar, and it will".to_string(),
                "  report back once the filter is in.".to_string(),
                right(hint),
                rule.clone(),
                "❯ ".to_string(),
                rule.clone(),
                "  Opus 5.5 | xhigh | Ctx 69% | $919.03 | 5h 7% · 7d 58%".to_string(),
                "  ff8d2575-b069-47b5-b375-69750cc8353f | …/Ghostex".to_string(),
                format!(
                    "  ⏵⏵ bypass permissions on (shift+tab to cycle){}{hint}",
                    " ".repeat(12)
                ),
                right(hint),
            ]
            .join("\n");
            let selection = crate::session_chat_options::detect_session_chat_selection(
                crate::session_chat_options::SessionChatOptionAgent::Claude,
                &screen,
            )
            .expect("the footer is read");
            let footer = selection.terminal_status_line.unwrap_or_default();
            assert!(footer.contains("Opus 5.5 | xhigh"), "{footer}");
            assert!(footer.contains("bypass permissions on"), "{footer}");
            assert!(
                !footer.contains("Update available"),
                "width {width}: {footer}"
            );

            let activity =
                crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                    Some("claude"),
                    &screen,
                )
                .expect("the streamed message is read");
            let text = activity.text.clone().unwrap_or_default();
            assert!(
                text.contains("report back once the filter is in."),
                "{text}"
            );
            assert!(!text.contains("Update available"), "width {width}: {text}");
            assert!(
                !activity.label.contains("Update available"),
                "width {width}"
            );
        }
    }
}
