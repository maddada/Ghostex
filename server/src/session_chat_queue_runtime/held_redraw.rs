//! A message held for an idle Claude session whose input box cannot be read: ask Claude to redraw.
//!
//! CDXC:SessionChat 2026-10-28 WHY:
//! A coordinator's thread reports sat as "Message from … · Waiting for agent" for fifteen minutes while the coordinator was idle (S90:P3lv0:G1pif, 2:50 to 3:05 PM). gxserver held the first report because "The claude input box is not on screen yet." and queued the next four behind it as startup sends. The scheduler re-read the screen every few seconds, through a whole turn the coordinator ran in between, and it never read as an input box. It went through only when the user opened the session, which resized and repainted Claude. The send path already answers such a screen with Claude's Ctrl+L redraw (`composer_repaint.rs`), but a held message never reaches the send path, and an idle, unwatched Claude never repaints by itself. So the scheduler asks for the same redraw. Only for a running Claude session: not mid-turn and not compacting; no notice, dialog or panel on screen; Claude's `❯` prompt row visible; the agent not still starting; and the input box unreadable for 20 s. Then at most once a minute, and every redraw is logged with the screen it saw (`sessionChatQueueRedrewClaude` in session-chat-send-failures.jsonl), so a frame that still does not parse can be read and fixed in the detector.
//!
//! CDXC:SessionChat 2026-10-29 WHY:
//! The same idle coordinator then showed a red Copy Report pill (user: "jsut got this?? pls fix why do i see this"). The chat's hover read of its screen had caught `❯` with no rules around it and a stray "48" under it, where the status line belongs; a minute later the screen read normally again. Two such frames are logged (2026-10-28 11:25 with the bottom rule erased, 2026-10-29 02:34 with both rules gone). Claude Code redraws only the cells it thinks changed, so when the grid is resized under it (the session is open in a terminal at 42 rows and in chat views at 24 rows, and zmx moves the grid between them), cells zmx reflowed differently stay wrong until a full repaint. The parser keeps requiring the rules around `❯`: a bare `❯` with no frame cannot be told apart from Claude's menus, and typing into a screen Ghostex cannot read is how messages get eaten. Instead the screen is repaired where it is read: the terminal-tail read behind the Terminal View light and Copy Report looks again after a moment, and only a frame still unreadable on an idle Claude gets the same Ctrl+L, sharing the once-a-minute limit (`repair_unreadable_frame`, logged as `sessionChatTailRedrewClaude`). A session nobody reads is left alone: sends repair their own screen (`composer_repaint.rs`) and held messages use the clock below.
//! SEE-ALSO: session_chat_send/composer_repaint.rs (the send path's redraw), scheduler.rs (the startup-send hold that calls the refresher), wiring.rs (`session_chat_queue_compacting_refresher`, which runs this after each fresh screen read), session_chat_composer.rs (`read_session_terminal_tail`, which runs `repair_unreadable_frame`), packages/gx-chat-core/src/extras/terminal_tail.rs (the chat's red verdict, which re-reads until it clears).

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::agents::identity::normalize_agent_id;
use crate::server::AppState;
use crate::session_chat_composer::SessionChatComposerReadiness;
use crate::session_chat_options::SessionChatTerminalDetection;

/// How long the input box must stay unreadable before the first redraw: a CLI mid-paint or still
/// booting settles well within it.
const FIRST_REDRAW_AFTER: Duration = Duration::from_secs(20);
/// The least time between two redraws of one session.
const REDRAW_EVERY: Duration = Duration::from_secs(60);

/// Claude's `Ctrl+L`, which repaints its whole screen and keeps the conversation and any typed text.
const CLAUDE_REDRAW: &str = "\u{c}";

#[derive(Clone, Copy)]
struct Clock {
    unreadable_since: Instant,
    last_redraw: Option<Instant>,
}

static CLOCKS: LazyLock<Mutex<HashMap<String, Clock>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether this fresh screen of a held session is one a redraw may fix: Claude, its input box
/// missing with nothing else in its place, its prompt row on screen, and not a CLI still starting.
pub(crate) fn screen_wants_redraw(
    agent: Option<&str>,
    detection: &SessionChatTerminalDetection,
) -> bool {
    // A background shell or monitor left running after the turn is not work (CDXC:SessionStatus
    // 2026-09-22 DECISION in presentation/session_attributes.rs), and its "· 1 shell still running"
    // line stays on an idle coordinator's screen for as long as the shell runs.
    detection.notice.is_none()
        && (detection.activity.is_none()
            || crate::session_chat_terminal_activity::is_session_chat_background_work_activity(
                detection.activity.as_ref(),
            ))
        && frame_wants_redraw(agent, &detection.composer)
}

/// The verdict half of [`screen_wants_redraw`]: a Claude input box missing, not a panel Escape
/// closes, `❯` on screen, and no agent still starting. `readiness` already carries the notice
/// detector's veto when it came from `session_composer_verdict`.
pub(crate) fn frame_wants_redraw(
    agent: Option<&str>,
    readiness: &SessionChatComposerReadiness,
) -> bool {
    normalize_agent_id(agent).as_deref() == Some("claude")
        && readiness.is_not_ready()
        && readiness.input_box_missing()
        && !readiness.should_dismiss()
        && readiness
            .screen_tail
            .iter()
            .any(|line| line.trim_start().starts_with('\u{276f}'))
        && !crate::session_chat_send_wake::screen_shows_agent_starting(&readiness.screen_tail)
}

/// Takes the session's once-a-minute redraw if it is free, whichever path asks.
fn claim_redraw(key: &str, now: Instant) -> bool {
    let Ok(mut clocks) = CLOCKS.lock() else {
        return false;
    };
    let clock = clocks.entry(key.to_string()).or_insert(Clock {
        unreadable_since: now,
        last_redraw: None,
    });
    if clock
        .last_redraw
        .is_some_and(|last| now.duration_since(last) < REDRAW_EVERY)
    {
        return false;
    }
    clock.last_redraw = Some(now);
    true
}

/// How long the tail read waits before looking again, so a frame Claude is still painting is not
/// repaired; then how long it waits for Claude's repaint to land.
const SETTLE_BEFORE_REPAIR: Duration = Duration::from_millis(600);
const REPAINT_SETTLE: Duration = Duration::from_millis(500);

/// The terminal-tail read's repair: when `readiness` is a Claude frame with no readable input box,
/// look again after [`SETTLE_BEFORE_REPAIR`]; if it still reads so and the session is idle, send
/// Claude's redraw (sharing the once-a-minute limit with held messages) and look once more.
/// Answers the capture and verdict to report, which are the given ones when nothing was done.
/// Blocking.
pub(crate) fn repair_unreadable_frame(
    session: &Value,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    agent: Option<&str>,
    first: (
        Option<crate::zmx::ZmxHistoryCapture>,
        SessionChatComposerReadiness,
    ),
) -> (
    Option<crate::zmx::ZmxHistoryCapture>,
    SessionChatComposerReadiness,
) {
    if !frame_wants_redraw(agent, &first.1) || !session_is_idle(session) {
        return first;
    }
    let look = || {
        let capture =
            crate::session_chat_composer::read_session_composer_verdict_capture(zmx_name, agent);
        let readiness = capture.as_ref().map_or_else(Default::default, |capture| {
            crate::session_chat_composer::session_composer_verdict(agent, &capture.text)
        });
        (capture, readiness)
    };
    std::thread::sleep(SETTLE_BEFORE_REPAIR);
    let again = look();
    if !frame_wants_redraw(agent, &again.1) {
        return again;
    }
    let key = crate::server::session_observer_key(project_id, session_id);
    if !claim_redraw(&key, Instant::now()) {
        return again;
    }
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatTailRedrewClaude",
        project_id,
        session_id,
        again.1.reason.as_deref().unwrap_or_default(),
        &again.1.screen_tail,
    );
    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatTailRedraw",
        "session-chat-message",
        CLAUDE_REDRAW,
    );
    if crate::zmx::session_chat_zmx_write(zmx_name, CLAUDE_REDRAW) != Ok(0) {
        return again;
    }
    std::thread::sleep(REPAINT_SETTLE);
    look()
}

/// Whether a redraw is due for a session whose input box has been unreadable since
/// `unreadable_since` and was last redrawn at `last_redraw`.
pub(crate) fn redraw_due(
    unreadable_since: Instant,
    last_redraw: Option<Instant>,
    now: Instant,
) -> bool {
    now.duration_since(unreadable_since) >= FIRST_REDRAW_AFTER
        && last_redraw.is_none_or(|last| now.duration_since(last) >= REDRAW_EVERY)
}

/// The session may take a message now: running, not mid-turn by its hooks, not compacting.
pub(crate) fn session_is_idle(session: &Value) -> bool {
    let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && crate::session_chat_compacting::session_chat_compacting_detected_at(session).is_none()
        && super::scheduler::session_chat_queue_activity_is_deliverable(session, &generated_at)
}

/// Runs after the scheduler's fresh screen read of a session holding a message: redraws Claude
/// when its unreadable input box has outlasted [`FIRST_REDRAW_AFTER`], at most every
/// [`REDRAW_EVERY`]. Blocking: it runs on the refresher's own blocking task.
pub(crate) fn redraw_held_claude_if_due(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    agent: Option<&str>,
    detection: &SessionChatTerminalDetection,
) {
    let key = crate::server::session_observer_key(project_id, session_id);
    let now = Instant::now();
    let Ok(mut clocks) = CLOCKS.lock() else {
        return;
    };
    if !screen_wants_redraw(agent, detection) {
        clocks.remove(&key);
        return;
    }
    let clock = *clocks.entry(key.clone()).or_insert(Clock {
        unreadable_since: now,
        last_redraw: None,
    });
    if !redraw_due(clock.unreadable_since, clock.last_redraw, now) {
        return;
    }
    let Some(session) = read_session(state, project_id, session_id) else {
        return;
    };
    if !session_is_idle(&session) {
        return;
    }
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    clocks.insert(
        key,
        Clock {
            last_redraw: Some(now),
            ..clock
        },
    );
    drop(clocks);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionChatQueueRedrewClaude",
        project_id,
        session_id,
        detection.composer.reason.as_deref().unwrap_or_default(),
        &detection.composer.screen_tail,
    );
    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        &zmx_name,
        "sessionChatQueueRedraw",
        "session-chat-message",
        CLAUDE_REDRAW,
    );
    let _ = crate::zmx::session_chat_zmx_write(&zmx_name, CLAUDE_REDRAW);
}

pub(super) fn read_session(state: &AppState, project_id: &str, session_id: &str) -> Option<Value> {
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .get_session(project_id, session_id)
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat_composer::detect_session_chat_composer_readiness;

    fn detection(screen: &str) -> SessionChatTerminalDetection {
        SessionChatTerminalDetection {
            composer: detect_session_chat_composer_readiness(Some("claude"), screen, None),
            ..SessionChatTerminalDetection::default()
        }
    }

    const RULE: &str =
        "────────────────────────────────────────────────────────────────────────────";

    /// The idle coordinator's prompt row with its box only half drawn (the bottom rule gone), as a
    /// capture caught it: no input box reads, Claude's `❯` is there.
    fn half_drawn() -> String {
        format!("⏺ Done.\n\n  22 tasks (20 done, 2 open)\n{RULE} Mr Big Chungus ─\n❯ \n")
    }

    #[test]
    fn an_idle_claude_screen_without_a_readable_box_is_redrawn() {
        let screen = half_drawn();
        let seen = detection(&screen);
        assert!(seen.composer.input_box_missing(), "{:?}", seen.composer);
        assert!(screen_wants_redraw(Some("claude"), &seen));
        // The coordinator runs as a custom agent whose composer is Claude's.
        assert!(!screen_wants_redraw(Some("codex"), &seen));
    }

    #[test]
    fn a_readable_box_a_starting_agent_or_no_prompt_row_is_left_alone() {
        let whole = format!("⏺ Done.\n{RULE} Mr Big Chungus ─\n❯ \n{RULE}\n");
        assert!(!screen_wants_redraw(Some("claude"), &detection(&whole)));
        let starting = "Restoring session...\n";
        assert!(!screen_wants_redraw(Some("claude"), &detection(starting)));
        let no_prompt = format!("⏺ Done.\n{RULE}\n");
        assert!(!screen_wants_redraw(Some("claude"), &detection(&no_prompt)));
    }

    #[test]
    fn a_redraw_waits_twenty_seconds_then_comes_at_most_once_a_minute() {
        let since = Instant::now();
        assert!(!redraw_due(since, None, since + Duration::from_secs(19)));
        assert!(redraw_due(since, None, since + Duration::from_secs(20)));
        let last = since + Duration::from_secs(20);
        assert!(!redraw_due(
            since,
            Some(last),
            last + Duration::from_secs(59)
        ));
        assert!(redraw_due(
            since,
            Some(last),
            last + Duration::from_secs(60)
        ));
    }

    /// The frame the coordinator's Copy Report read on 2026-10-29 at 02:34 (S90:P3lv0:G1pif): its
    /// reply, the task list, `❯` with no rule above or below it, and a stray "48" left where the
    /// status line was, above the permissions line.
    fn copy_report_frame() -> String {
        [
            "\u{273b} Cooked for 9s \u{b7} done 2:33 AM",
            "  22 tasks (21 done, 1 open)",
            "  \u{25fb} When 10.39.1 ships: tell G2z0a the mise fix is out (for Sam)",
            "   \u{2026} +17 completed",
            "",
            "",
            "\u{276f} ",
            "",
            "                                      48",
            "  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} 2 feedback drafts",
        ]
        .join("\n")
    }

    /// The frame the delivery watch saved on 2026-10-28 at 11:25: the top rule drawn, the bottom
    /// rule erased to a blank row above the status lines.
    fn delivery_notice_frame() -> String {
        format!(
            "  \u{2026} +17 completed\n{RULE}\n\u{276f}\n\n  Opus 5.5 | xhigh | Ctx 40% | $877.86 | 5h 7% \u{b7} 7d 51%\n  ff8d2575-b069-47b5-b375-69750cc8353f | \u{2026}/Ghostex\n  \u{23f5}\u{23f5} bypass permissions on (shift+tab to cycle) \u{b7} 2 feedback drafts\n"
        )
    }

    /// Both logged half-drawn frames stay "no input box" for the parser (a bare `❯` with no frame
    /// is not proof of Claude's prompt), and both are frames the guarded redraw repairs.
    #[test]
    fn the_logged_half_drawn_frames_are_repaired_not_trusted() {
        for screen in [copy_report_frame(), delivery_notice_frame()] {
            let verdict =
                crate::session_chat_composer::session_composer_verdict(Some("claude"), &screen);
            assert!(verdict.is_not_ready(), "{screen}");
            assert_eq!(
                verdict.reason.as_deref(),
                Some("The claude input box is not on screen yet.")
            );
            assert!(frame_wants_redraw(Some("claude"), &verdict), "{screen}");
            assert!(screen_wants_redraw(Some("claude"), &detection(&screen)));
        }
        // The repaired frame reads as an input box again.
        let repaired =
            format!("{RULE} Main Coordinator \u{2500}\n\u{276f} \n{RULE}\n  Opus 5.5 | xhigh\n");
        let verdict =
            crate::session_chat_composer::session_composer_verdict(Some("claude"), &repaired);
        assert!(!verdict.is_not_ready());
        assert!(!frame_wants_redraw(Some("claude"), &verdict));
    }

    /// An idle coordinator whose finished turn left a background shell running: the footer's
    /// "· 1 shell still running" is not work, so its unreadable box is still redrawn. A compaction
    /// on screen is, and is left alone.
    #[test]
    fn a_background_shell_line_does_not_stop_the_redraw() {
        let screen = format!(
            "\u{273b} Crunched for 18s \u{b7} done 2:19 PM \u{b7} 1 shell still running\n\n{}",
            copy_report_frame()
        );
        let mut seen = detection(&screen);
        seen.activity =
            crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                Some("claude"),
                &screen,
            );
        assert!(
            crate::session_chat_terminal_activity::is_session_chat_background_work_activity(
                seen.activity.as_ref()
            ),
            "{:?}",
            seen.activity
        );
        assert!(screen_wants_redraw(Some("claude"), &seen));

        let compacting = format!(
            "\u{273b} Compacting conversation\u{2026}\n\n{}",
            copy_report_frame()
        );
        let mut seen = detection(&compacting);
        seen.activity =
            crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                Some("claude"),
                &compacting,
            );
        assert!(seen.activity.is_some(), "{compacting}");
        assert!(!screen_wants_redraw(Some("claude"), &seen));
    }

    #[test]
    fn redraws_share_one_limit_per_session() {
        let now = Instant::now();
        let key = "test-project:test-session-limit";
        assert!(claim_redraw(key, now));
        assert!(!claim_redraw(key, now + Duration::from_secs(59)));
        assert!(claim_redraw(key, now + Duration::from_secs(60)));
    }
}
