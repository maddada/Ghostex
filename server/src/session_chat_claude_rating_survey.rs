//! Claude Code's optional rating survey, which gxserver dismisses with its own `0` key when a
//! message has to go in and the survey has pushed the input box off the screen.
//!
//! Claude Code 2.1.311 draws it under the transcript, over the input box, once a turn ends:
//!
//! ```text
//! ● How is Claude doing this session? (optional)
//!   1: Bad    2: Fine   3: Good   0: Dismiss
//!                                     Update available! Run: mise upgrade claude
//! ──────────────────────────────────────────────────────────────────────────────
//! ❯
//! ```
//!
//! On the 24-row grid a hidden session rests at, a tall turn footer (a task list) and the survey
//! push the input box below the bottom row, so the screen ends at the survey.

use std::time::Duration;

use crate::session_chat_claude_feedback_card::{is_rule, screen_lines};

/// The questions Claude asks with the survey's rating row (`Fq` and the long-context variant in
/// its source; both draw the same row and read the same keys).
const SURVEY_TITLES: &[&str] = &[
    "How is Claude doing this session? (optional)",
    "How well is Claude following the instructions you gave earlier in this conversation? (optional)",
];

/// The rating row's `key: label` pairs, in order. Claude adds "4: Unsure" before Dismiss in some
/// variants.
const RATINGS: &[(&str, &str)] = &[("1:", "Bad"), ("2:", "Fine"), ("3:", "Good")];
const UNSURE: (&str, &str) = ("4:", "Unsure");
const DISMISS: (&str, &str) = ("0:", "Dismiss");

/// Rows Claude may draw between the survey and the bottom of a screen that cut the input box off
/// ("Update available! Run: …", and the input box's top rule).
const ROWS_AFTER_SURVEY: usize = 2;

/// The least indent of Claude's notice row; the rating row is indented two.
const NOTICE_INDENT: usize = 3;

/// The survey's Dismiss: no rating is recorded and the survey closes at once (`"0":"dismissed"`,
/// then the survey state `closed`, in Claude's source).
pub(crate) const SURVEY_DISMISS_KEY: &str = "0";

/// Claude ignores digits for 600 ms after the survey appears and the `0` would stay in the input
/// box as text, so a survey is dismissed only once it has been on screen longer than that.
pub(crate) const SURVEY_SETTLE: Duration = Duration::from_millis(800);

/// CDXC:SessionChat 2026-10-30 WHY:
/// The coordinator (S90:P3lv0:G1pif) sat idle from 03:09 to 10:32 UTC with a thread report queued, because Claude Code's "How is Claude doing this session?" survey had pushed its input box below the bottom of the 24-row hidden grid, and the queue never sends into an input box it cannot see. The survey reads the input box's own value: one valid digit alone in it answers after a 400 ms pause, and any other text closes the survey by itself. So `0` alone, written only while the survey is the bottom of the screen and has been up past its 600 ms mount delay, dismisses it (never a rating), the input box comes back into view, and the send's verified clear runs before the message is pasted, so a `0` the survey did not take is cleared rather than sent. When the input box is still visible under the survey, the message is typed as usual and Claude closes the survey itself.
/// WHY: matched by the survey's own two lines at the bottom of the screen with nothing but Claude's right-aligned notices (and the input box's cut-off top rule) after them, so a question, a permission prompt or the survey quoted in the transcript never reads as it.
/// SEE-ALSO: server/src/session_chat_composer.rs (the survey makes the input box dismissible for a send), server/src/session_chat_send/worker.rs (`DismissClaudePanel` writes the key).
pub(crate) fn rating_survey_on_screen(screen_text: &str) -> bool {
    let lines = screen_lines(screen_text);
    let Some(title) = lines.iter().rposition(|line| is_survey_title(line)) else {
        return false;
    };
    lines.get(title + 1).is_some_and(|row| is_rating_row(row))
        && input_box_cut_off(&lines[title + 2..])
}

/// `● How is Claude doing this session? (optional)` from the first column, whatever glyph Claude
/// draws first.
fn is_survey_title(line: &str) -> bool {
    let Some((glyph, rest)) = line.split_once(' ') else {
        return false;
    };
    glyph.chars().count() == 1
        && !glyph.starts_with(char::is_alphanumeric)
        && SURVEY_TITLES.contains(&rest.trim())
}

/// `  1: Bad    2: Fine   3: Good   0: Dismiss`, indented under the title.
fn is_rating_row(row: &str) -> bool {
    if !row.starts_with("  ") || row.starts_with("   ") {
        return false;
    }
    let tokens: Vec<&str> = row.split_whitespace().collect();
    let pairs: Vec<(&str, &str)> = tokens
        .chunks(2)
        .filter_map(|pair| match pair {
            [key, label] => Some((*key, *label)),
            _ => None,
        })
        .collect();
    if pairs.len() * 2 != tokens.len() {
        return false;
    }
    let mut expected: Vec<(&str, &str)> = RATINGS.to_vec();
    if pairs.len() == RATINGS.len() + 2 {
        expected.push(UNSURE);
    }
    expected.push(DISMISS);
    pairs == expected
}

/// Only Claude's notice row follows the survey, and at most the input box's top rule as the very
/// last row: the input box itself is below the bottom of the screen.
fn input_box_cut_off(after: &[String]) -> bool {
    if after.len() > ROWS_AFTER_SURVEY {
        return false;
    }
    match after.split_last() {
        None => true,
        Some((last, rest)) => {
            rest.iter().all(|line| is_notice_row(line)) && (is_notice_row(last) || is_rule(last))
        }
    }
}

/// The row Claude right-aligns over its input box ("◐ medium · /effort", "You've used 90% of your
/// weekly limit · … · Update available! Run: …"). A long notice fills most of the row, so it is
/// told by its indent, deeper than the rating row's, and by not opening like transcript text.
fn is_notice_row(line: &str) -> bool {
    let text = line.trim_start();
    line.len() - text.len() >= NOTICE_INDENT
        && !text.starts_with(['-', '*', '•', '⎿', '│', '●', '❯'])
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bottom of the coordinator's screen on 2026-10-30, as captured in every
    /// `sessionChatQueueResizedForRepaint` line from 03:10 to 10:32 UTC.
    const STUCK_SCREEN: &str = "  🔄 In progress:\n  - Build unified sidebar view: It's bringing the desktop Unified sidebar to the phone. When it's done, I'll check it, commit it, and start the 10.41.0 TestFlight build.\n  - Incoming Issues Manager: Still posting the four Discord release messages.\n✻ Cooked for 27s · done 6:43 AM\n  29 tasks (27 done, 1 in progress, 1 open)\n  ◼ After 10.41.0 ships: message everyone waiting on what was released\n  ◻ Phone Unified sidebar (G0ju4), then iOS TestFlight 10.41.0 to Additional, then tell Bagheera\n  ✔ Commit Super+C, AppImage self-update and web build fix (G1wn2)\n  ✔ Test Claude agents end to end on Omarchy and fix issues\n  ✔ Commit agent mentions, Copy ID and pills (G1noe)\n   … +24 completed\n● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n                                                                                                                                                            Update available! Run: mise upgrade claude";

    /// Claude Code 2.1.311 with the survey forced (`CLAUDE_FORCE_DISPLAY_SURVEY=1`) on a grid shrunk
    /// until the survey pushed its input box off the bottom: 10, 8 and 6 rows.
    const LIVE_10_ROWS: &str = "\n ▐▛███▛█   Claude Code v2.1.311\n▝▜██████▀  Haiku 5.5 with medium effort · Claude Max\n ▝▝   ▝▝   ~/.local/share/agent-work/ghostex/issues-1027/survey-queue/proj\n\n\n● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n                                       You've used 90% of your weekly limit · resets Nov 1 at 2pm (Asia/Dubai) · /usage-credits to keep using Claude Code · Update available! Run: mise upgrade claude\n────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";
    const LIVE_8_ROWS: &str = " ▐▛███▛█   Claude Code v2.1.311\n▝▜██████▀  Haiku 5.5 with medium effort · Claude Max\n ▝▝   ▝▝   ~/.local/share/agent-work/ghostex/issues-1027/survey-queue/proj\n\n\n● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n                                       You've used 90% of your weekly limit · resets Nov 1 at 2pm (Asia/Dubai) · /usage-credits to keep using Claude Code · Update available! Run: mise upgrade claude";
    const LIVE_6_ROWS: &str = "▝▜██████▀  Haiku 5.5 with medium effort · Claude Max\n ▝▝   ▝▝   ~/.local/share/agent-work/ghostex/issues-1027/survey-queue/proj\n\n\n● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss";
    /// Another 10-row run, whose notice row is the short effort hint, and the same grid 1.5 s
    /// after `0` was typed: the survey is gone and the empty input box is back on screen.
    const LIVE_10_ROWS_EFFORT: &str = "\n ▐▛███▛█   Claude Code v2.1.311\n▝▜██████▀  Haiku 5.5 with medium effort · Claude Max\n ▝▝   ▝▝   ~/.local/share/agent-work/ghostex/issues-1027/survey-queue/proj\n\n\n● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n                                                                                                                                                                                    ◐ medium · /effort\n────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";
    /// An 8-row grid (the survey's rows end at the notice, as on the coordinator's screen) after
    /// `0`: the three rows the survey gave back are exactly the input box.
    const LIVE_8_ROWS_AFTER_DISMISS: &str = " ▐▛███▛█   Claude Code v2.1.311\n▝▜██████▀  Haiku 5.5 with medium effort · Claude Max\n ▝▝   ▝▝   ~/.local/share/agent-work/ghostex/issues-1027/survey-queue/proj\n\n                                                                                                                                                                                    ◐ medium · /effort\n────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n❯\u{a0}\n────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";
    const LIVE_10_ROWS_AFTER_DISMISS: &str = "\n ▐▛███▛█   Claude Code v2.1.311\n▝▜██████▀  Haiku 5.5 with medium effort · Claude Max\n ▝▝   ▝▝   ~/.local/share/agent-work/ghostex/issues-1027/survey-queue/proj\n\n                                                                                                                                                                                    ◐ medium · /effort\n────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n❯\u{a0}\n────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n  Haiku 5.5 | medium | $0.00";

    fn rule() -> String {
        "─".repeat(120)
    }

    #[test]
    fn the_survey_that_held_the_coordinator_for_seven_hours_is_recognised() {
        assert!(rating_survey_on_screen(STUCK_SCREEN));
        // The same screen with the input box's top rule cut off at the bottom row.
        assert!(rating_survey_on_screen(&format!(
            "{STUCK_SCREEN}\n{}",
            rule()
        )));
        for live in [LIVE_10_ROWS, LIVE_8_ROWS, LIVE_6_ROWS, LIVE_10_ROWS_EFFORT] {
            assert!(rating_survey_on_screen(live), "{live}");
        }
        for dismissed in [LIVE_10_ROWS_AFTER_DISMISS, LIVE_8_ROWS_AFTER_DISMISS] {
            assert!(!rating_survey_on_screen(dismissed));
            let ready = crate::session_chat_composer::detect_session_chat_composer_ready(
                Some("claude"),
                dismissed,
            );
            assert_eq!(
                ready.state,
                crate::session_chat_composer::SessionChatComposerState::Ready,
                "{dismissed}"
            );
            // The `0` was taken by the survey, not typed into the box.
            assert!(
                crate::session_chat_composer::session_chat_composer_input("claude", dismissed)
                    .is_some_and(|input| input.is_empty())
            );
        }
        // Without the update notice, and with ANSI colour as zmx captures it.
        assert!(rating_survey_on_screen(
            "✻ Cooked for 27s\n\u{1b}[36m●\u{1b}[0m \u{1b}[1mHow is Claude doing this session? (optional)\u{1b}[0m\n  \u{1b}[36m1\u{1b}[0m: Bad    \u{1b}[36m2\u{1b}[0m: Fine   \u{1b}[36m3\u{1b}[0m: Good   \u{1b}[36m0\u{1b}[0m: Dismiss\n"
        ));
        // The variant with Unsure, and the long-context question.
        assert!(rating_survey_on_screen(
            "● How well is Claude following the instructions you gave earlier in this conversation? (optional)\n  1: Bad    2: Fine   3: Good   4: Unsure 0: Dismiss"
        ));
    }

    #[test]
    fn a_survey_above_a_visible_input_box_is_left_to_the_message() {
        // Typing the message closes the survey by itself, so nothing is dismissed.
        let screen = format!(
            "{STUCK_SCREEN}\n{}\n❯ \n{}\n  ⏵⏵ auto mode on",
            rule(),
            rule()
        );
        assert!(!rating_survey_on_screen(&screen));
        assert_eq!(
            crate::session_chat_composer::detect_session_chat_composer_ready(
                Some("claude"),
                &screen
            )
            .state,
            crate::session_chat_composer::SessionChatComposerState::Ready
        );
    }

    #[test]
    fn the_survey_screen_is_closable_for_a_send_and_no_question_for_the_user() {
        use crate::session_chat_composer::{
            detect_session_chat_composer_readiness, SessionChatComposerState,
        };
        let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
            Some("claude"),
            STUCK_SCREEN,
        );
        // No card the user must answer and nothing that fails the queued row.
        assert!(notice
            .as_ref()
            .is_none_or(|notice| !notice.is_answerable() && !notice.blocks_queued_delivery()));
        let readiness =
            detect_session_chat_composer_readiness(Some("claude"), STUCK_SCREEN, notice.as_ref());
        assert_eq!(readiness.state, SessionChatComposerState::NotReady);
        // The queue delivers it at once and the send closes the survey before typing.
        assert!(readiness.should_dismiss(), "{readiness:?}");
        assert!(!readiness.input_box_missing());
    }

    #[test]
    fn a_permission_prompt_or_question_at_the_bottom_is_not_closed_for_a_send() {
        use crate::session_chat_composer::detect_session_chat_composer_readiness;
        for screen in [
            "✻ Cooked for 27s\n● Bash(rm -rf build)\nBash command\n  rm -rf build\nDo you want to proceed?\n❯ 1. Yes\n  2. Yes, and don't ask again for rm commands in /tmp/x\n  3. No, and tell Claude what to do differently (esc)",
            "● How is Claude doing this session? (optional)\n❯ 1. Bad\n  2. Fine\n  3. Good\n  4. Type something.\nEnter to select · ↑/↓ to navigate · Esc to cancel",
        ] {
            let notice =
                crate::session_chat_notice::classify_session_chat_terminal_notice(Some("claude"), screen);
            let readiness =
                detect_session_chat_composer_readiness(Some("claude"), screen, notice.as_ref());
            assert!(!readiness.should_dismiss(), "{screen}: {readiness:?}");
        }
    }

    #[test]
    fn questions_permission_prompts_and_quotes_are_never_the_survey() {
        // Claude's AskUserQuestion with numbered options.
        assert!(!rating_survey_on_screen(
            "● How is Claude doing this session? (optional)\n❯ 1. Bad\n  2. Fine\n  3. Good\n  4. Type something."
        ));
        // A permission prompt at the bottom of the screen.
        assert!(!rating_survey_on_screen(
            "Bash command\n  rm -rf build\nDo you want to proceed?\n❯ 1. Yes\n  2. Yes, and don't ask again for rm commands\n  3. No, and tell Claude what to do differently (esc)"
        ));
        // The survey quoted in an assistant reply, with the reply going on under it.
        assert!(!rating_survey_on_screen(
            "● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n  That survey sits where the input box goes.\n"
        ));
        // Quoted inside a tool result (indented), at the bottom of a cut-off screen.
        assert!(!rating_survey_on_screen(
            "  ⎿  ● How is Claude doing this session? (optional)\n       1: Bad    2: Fine   3: Good   0: Dismiss"
        ));
        // Claude's typed-over form, which takes Ctrl+X chords, not digits.
        assert!(!rating_survey_on_screen(
            "● How is Claude doing this session? (optional)\n  to rate, press ctrl+x and then 1: Bad  2: Fine  3: Good"
        ));
        // A different question with the same row.
        assert!(!rating_survey_on_screen(
            "● Should I delete the old branch?\n  1: Bad    2: Fine   3: Good   0: Dismiss"
        ));
        // A row with a rating missing.
        assert!(!rating_survey_on_screen(
            "● How is Claude doing this session? (optional)\n  1: Bad    3: Good   0: Dismiss"
        ));
    }
}
