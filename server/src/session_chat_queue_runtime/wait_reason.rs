//! Why the head of a session's queue has not gone out, in the words its row shows, and the
//! thresholds of the ladder that heals a row waiting on an agent that is ready by every other sign.
//!
//! SEE-ALSO: scheduler.rs (records a reason at every gate, runs the ladder and the watchdog),
//! held_redraw.rs (the Ctrl+L step), repaint_nudge.rs (the resize step), session_chat_queue.rs
//! (`set_queue_wait_reason`, which puts the words on the row), packages/gx-chat-core
//! (`waitReason` on queue rows, held sends and agent message cards).

use chrono::{DateTime, Utc};

/// A row's reason shows once it has waited this long: shorter waits are a normal turn ending.
pub(super) const WAIT_REASON_SHOWN_AFTER_MS: i64 = 60_000;

/// The input-box ladder's Ctrl+L step (held_redraw.rs owns the clock; kept here for the words).
pub(super) const INPUT_BOX_REDRAW_AFTER_MS: i64 = 20_000;

/// The input-box ladder's last resort: by now the fresh reads, two Ctrl+L redraws and a resize
/// nudge have had their chance, so the row goes to the send path, whose own capture decides.
pub(super) const INPUT_BOX_LAST_RESORT_AFTER_MS: i64 = 180_000;

/// The watchdog: a head row held this long by a gate that should have let it go, on a session
/// idle by every signal, is logged and handed to the same ladder's last resort.
pub(super) const WATCHDOG_AFTER_MS: i64 = 300_000;

/// CDXC:SessionChat 2026-10-30 WHY:
/// A thread report waited seven hours behind Claude Code's rating survey with its row saying "trying to send anyway" the whole time, and nobody was told: the coordinator it was for was the session stuck, and the user was away. A held message keeps retrying for as long as the session lives (the 2026-10-11 DECISION in session_chat_send_wake.rs), so the retries stay; what changes after this long is the row's words, which say what the terminal shows and what to do, and one notification in the bell, so a stuck message is never silent.
pub(super) const INPUT_BOX_STUCK_AFTER_MS: i64 = 300_000;

/// Longest piece of one screen row quoted in a stuck row's words.
const STUCK_SCREEN_ROW_CHARS: usize = 90;
/// Screen rows quoted: the bottom of what the agent shows where its input box should be.
const STUCK_SCREEN_ROWS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InputBoxStep {
    /// Fresh screen reads on the startup send's backoff.
    Reading,
    /// Ctrl+L (Claude) and the resize nudge (every agent) are being tried.
    Repainting,
    /// The send path is trying; it gives up with Not sent after its two-minute hold window.
    SendingAnyway,
}

impl InputBoxStep {
    pub(super) fn after(waited_ms: i64) -> Self {
        if waited_ms >= INPUT_BOX_LAST_RESORT_AFTER_MS {
            Self::SendingAnyway
        } else if waited_ms >= INPUT_BOX_REDRAW_AFTER_MS {
            Self::Repainting
        } else {
            Self::Reading
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WaitReason {
    Asleep,
    Closed,
    ModelChange,
    ModelChangeFailed,
    AccountRecovery,
    MissingLogin,
    StartFailed,
    Starting,
    Compacting,
    Working,
    Answer,
    Settling,
    InputBox(InputBoxStep),
    /// The send path found no input box a moment ago and put the row back.
    Retrying,
}

impl WaitReason {
    /// The sentence the row shows.
    pub(super) fn text(&self) -> &'static str {
        match self {
            Self::Asleep => "Waiting for the session to wake up.",
            Self::Closed => "Waiting for the session to be opened again.",
            Self::ModelChange => "Waiting for the model change before it to apply.",
            Self::ModelChangeFailed => {
                "Waiting: the model change before this message failed. Pick a model again to send it."
            }
            Self::AccountRecovery => "Waiting for the agent's account to be ready again.",
            Self::MissingLogin => "Waiting for the agent's account to be signed in.",
            Self::StartFailed => "Waiting: the agent couldn't start. Start it again to send this.",
            Self::Starting => "Waiting for the agent to start.",
            Self::Compacting => "Waiting for the agent to finish compacting the conversation.",
            Self::Working => "Waiting for the agent to finish its turn.",
            Self::Answer => "Waiting for you to answer the agent's question.",
            Self::Settling => "Waiting for the agent to settle after its turn.",
            Self::InputBox(InputBoxStep::Reading) => {
                "Waiting: the agent's input box can't be read yet, trying again."
            }
            Self::InputBox(InputBoxStep::Repainting) => {
                "Waiting: the agent's input box can't be read yet, asking it to redraw its screen."
            }
            Self::InputBox(InputBoxStep::SendingAnyway) | Self::Retrying => {
                "Waiting: the agent's input box can't be read yet, trying to send anyway."
            }
        }
    }

    /// A stable name for logs; a change of kind restarts the watchdog's clock.
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Asleep => "asleep",
            Self::Closed => "closed",
            Self::ModelChange => "modelChange",
            Self::ModelChangeFailed => "modelChangeFailed",
            Self::AccountRecovery => "accountRecovery",
            Self::MissingLogin => "missingLogin",
            Self::StartFailed => "startFailed",
            Self::Starting => "starting",
            Self::Compacting => "compacting",
            Self::Working => "working",
            Self::Answer => "answer",
            Self::Settling => "settling",
            Self::InputBox(_) | Self::Retrying => "inputBox",
        }
    }

    /// Part of the input-box ladder, whose clock must survive the send path's retry spacing.
    pub(super) fn is_input_box(&self) -> bool {
        matches!(self, Self::InputBox(_) | Self::Retrying)
    }

    /// Whether the watchdog may hand the row to the send path once every other signal says the
    /// agent is idle. The other reasons are states a person or another process must end (a
    /// question, a sign-in, a model pick, a closed or sleeping session, a booting agent), each
    /// already shown on the row; the send path's own guards apply either way.
    pub(super) fn escalates(&self) -> bool {
        matches!(self, Self::Settling | Self::InputBox(_) | Self::Retrying)
    }
}

/// How long the current head row has waited, and for what kind of reason since when.
#[derive(Clone, Debug)]
pub(super) struct HeadWait {
    pub(super) prompt_id: String,
    pub(super) first_seen: DateTime<Utc>,
    pub(super) kind: &'static str,
    pub(super) kind_since: DateTime<Utc>,
    pub(super) watchdog_logged: bool,
    /// The row's stuck notification went out (once per row and kind of wait).
    pub(super) stuck_reported: bool,
}

impl HeadWait {
    /// Moves the clock to `prompt_id` held for `reason` at `now` (a new head row starts a new
    /// clock). Answers the words the row should show: none until it has waited
    /// [`WAIT_REASON_SHOWN_AFTER_MS`].
    pub(super) fn observe(
        wait: &mut Option<HeadWait>,
        prompt_id: &str,
        reason: &WaitReason,
        now: DateTime<Utc>,
    ) -> Option<&'static str> {
        let wait = match wait {
            Some(wait) if wait.prompt_id == prompt_id => wait,
            _ => wait.insert(HeadWait {
                prompt_id: prompt_id.to_string(),
                first_seen: now,
                kind: reason.kind(),
                kind_since: now,
                watchdog_logged: false,
                stuck_reported: false,
            }),
        };
        if wait.kind != reason.kind() {
            wait.kind = reason.kind();
            wait.kind_since = now;
            wait.watchdog_logged = false;
            wait.stuck_reported = false;
        }
        (now.signed_duration_since(wait.first_seen)
            .num_milliseconds()
            >= WAIT_REASON_SHOWN_AFTER_MS)
            .then(|| reason.text())
    }

    pub(super) fn kind_waited_ms(&self, now: DateTime<Utc>) -> i64 {
        now.signed_duration_since(self.kind_since)
            .num_milliseconds()
    }

    /// Minutes the row has waited on the input box once that is past
    /// [`INPUT_BOX_STUCK_AFTER_MS`], `None` before then or for any other kind of wait.
    pub(super) fn stuck_minutes(&self, reason: &WaitReason, now: DateTime<Utc>) -> Option<i64> {
        let waited = self.kind_waited_ms(now);
        (reason.is_input_box() && waited >= INPUT_BOX_STUCK_AFTER_MS).then_some(waited / 60_000)
    }
}

/// The bottom of the screen the agent shows instead of its input box, quoted: its last rows that
/// are not a right-aligned notice ("Update available! …") or a bare rule.
fn screen_excerpt(screen_tail: &[String]) -> Option<String> {
    let mut rows: Vec<String> = screen_tail
        .iter()
        .rev()
        .filter(|line| {
            let trimmed = line.trim();
            let lead = line.len() - line.trim_start().len();
            !trimmed.is_empty()
                && lead * 3 < line.chars().count()
                && !trimmed
                    .chars()
                    .all(|ch| matches!(ch, '─' | '━' | '═' | '-'))
        })
        .take(STUCK_SCREEN_ROWS)
        .map(|line| {
            let row = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if row.chars().count() <= STUCK_SCREEN_ROW_CHARS {
                row
            } else {
                let mut cut: String = row.chars().take(STUCK_SCREEN_ROW_CHARS - 1).collect();
                cut.push('…');
                cut
            }
        })
        .collect();
    rows.reverse();
    (!rows.is_empty()).then(|| rows.join(" / "))
}

fn minutes(count: i64) -> String {
    if count == 1 {
        "1 minute".to_string()
    } else {
        format!("{count} minutes")
    }
}

/// What a row stuck on the input box says, instead of "trying to send anyway" for good.
pub(super) fn stuck_input_box_text(waited_minutes: i64, screen_tail: &[String]) -> String {
    let shows = screen_excerpt(screen_tail)
        .map(|excerpt| format!(" The terminal shows: \u{201c}{excerpt}\u{201d}."))
        .unwrap_or_default();
    format!(
        "Not sent yet: the agent's input box hasn't been on screen for {}.{shows} Open the terminal to clear what is in the way; Ghostex keeps trying and sends it once the box is back.",
        minutes(waited_minutes)
    )
}

/// The bell notification for a row stuck on the input box.
pub(super) fn stuck_notification_body(waited_minutes: i64, screen_tail: &[String]) -> String {
    let shows = screen_excerpt(screen_tail)
        .map(|excerpt| format!(" The terminal shows: \u{201c}{excerpt}\u{201d}."))
        .unwrap_or_default();
    format!(
        "A message has waited {} to reach this agent because its input box isn't on screen.{shows} Open the terminal to clear it.",
        minutes(waited_minutes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + seconds, 0).unwrap()
    }

    #[test]
    fn a_reason_shows_after_a_minute_and_follows_the_ladder() {
        let mut wait = None;
        let working = WaitReason::Working;
        assert_eq!(HeadWait::observe(&mut wait, "Q1", &working, at(0)), None);
        assert_eq!(HeadWait::observe(&mut wait, "Q1", &working, at(59)), None);
        assert_eq!(
            HeadWait::observe(&mut wait, "Q1", &working, at(60)),
            Some("Waiting for the agent to finish its turn.")
        );
        // The turn ends and the box cannot be read: the words move with the ladder at once.
        let reading = WaitReason::InputBox(InputBoxStep::after(0));
        assert_eq!(
            HeadWait::observe(&mut wait, "Q1", &reading, at(61)),
            Some("Waiting: the agent's input box can't be read yet, trying again.")
        );
        assert_eq!(wait.as_ref().unwrap().kind_waited_ms(at(61)), 0);
        let anyway = WaitReason::InputBox(InputBoxStep::after(180_000));
        assert_eq!(
            HeadWait::observe(&mut wait, "Q1", &anyway, at(241)),
            Some("Waiting: the agent's input box can't be read yet, trying to send anyway.")
        );
        // The send path's retry spacing is the same kind, so the watchdog clock keeps running.
        HeadWait::observe(&mut wait, "Q1", &WaitReason::Retrying, at(245));
        assert_eq!(wait.as_ref().unwrap().kind_waited_ms(at(245)), 184_000);
        // The next head row starts its own minute.
        assert_eq!(HeadWait::observe(&mut wait, "Q2", &working, at(300)), None);
    }

    #[test]
    fn a_row_stuck_on_the_input_box_says_what_the_terminal_shows() {
        let mut wait = None;
        let anyway = WaitReason::InputBox(InputBoxStep::SendingAnyway);
        HeadWait::observe(&mut wait, "Q1", &anyway, at(0));
        let head = wait.as_ref().unwrap();
        assert_eq!(head.stuck_minutes(&anyway, at(299)), None);
        assert_eq!(head.stuck_minutes(&anyway, at(300)), Some(5));
        assert_eq!(
            head.stuck_minutes(&WaitReason::Retrying, at(26_400)),
            Some(440)
        );
        assert_eq!(head.stuck_minutes(&WaitReason::Working, at(26_400)), None);
        // The tail the coordinator's screen showed on 2026-10-30.
        let screen: Vec<String> = [
            "✻ Cooked for 27s · done 6:43 AM",
            "   … +24 completed",
            "● How is Claude doing this session? (optional)",
            "  1: Bad    2: Fine   3: Good   0: Dismiss",
            &format!(
                "{}Update available! Run: mise upgrade claude",
                " ".repeat(150)
            ),
        ]
        .iter()
        .map(|line| line.to_string())
        .collect();
        assert_eq!(
            stuck_input_box_text(440, &screen),
            "Not sent yet: the agent's input box hasn't been on screen for 440 minutes. The terminal shows: \u{201c}● How is Claude doing this session? (optional) / 1: Bad 2: Fine 3: Good 0: Dismiss\u{201d}. Open the terminal to clear what is in the way; Ghostex keeps trying and sends it once the box is back."
        );
        assert_eq!(
            stuck_notification_body(5, &[]),
            "A message has waited 5 minutes to reach this agent because its input box isn't on screen. Open the terminal to clear it."
        );
    }

    #[test]
    fn the_ladder_steps_and_what_the_watchdog_may_escalate() {
        assert_eq!(InputBoxStep::after(19_999), InputBoxStep::Reading);
        assert_eq!(InputBoxStep::after(20_000), InputBoxStep::Repainting);
        assert_eq!(InputBoxStep::after(179_999), InputBoxStep::Repainting);
        assert_eq!(InputBoxStep::after(180_000), InputBoxStep::SendingAnyway);
        for reason in [
            WaitReason::Settling,
            WaitReason::Retrying,
            WaitReason::InputBox(InputBoxStep::Reading),
        ] {
            assert!(reason.escalates(), "{reason:?}");
        }
        for reason in [
            WaitReason::Working,
            WaitReason::Answer,
            WaitReason::Compacting,
            WaitReason::Asleep,
            WaitReason::Closed,
            WaitReason::Starting,
            WaitReason::MissingLogin,
            WaitReason::ModelChangeFailed,
        ] {
            assert!(!reason.escalates(), "{reason:?}");
        }
    }
}
