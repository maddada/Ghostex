//! When a chat Stop may press Ctrl+C in a Hermes Agent terminal.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// Hermes force-quits on a second Ctrl+C within 2 s of the one that interrupted; the rest absorbs
/// queue and PTY jitter between two writes.
const HERMES_INTERRUPT_SPACING: Duration = Duration::from_millis(3_000);

/// The input area sits at the bottom of the screen; the hint is never further up than this.
const HERMES_BUSY_HINT_SCAN_LINES: usize = 12;

/// CDXC:SessionChat 2026-10-17 WHY:
/// Hermes Agent ignores Escape during a turn (a single Escape is only the Alt-key prefix, a double one discards the draft), so chat Stop and `ghostex interrupt-session-chat` answered "interrupted" while the turn kept running. Ctrl+C stops it, but only when the turn is running and the input line is empty: with a draft it clears the draft, at an idle empty prompt it quits Hermes, and a second press within 2 s of the one that interrupted force-quits it. Hermes shows exactly that state as its input placeholder ("msg=interrupt · /queue · /bg · /steer · Ctrl+C cancel"; the command names stay English in every locale), so Stop presses Ctrl+C only while the placeholder shows, and at most once every three seconds.
/// CDXC:HermesGateway 2026-11-05 WHY:
/// Hermes 0.22.5 rewrote that placeholder per busy mode and translated it ("type to redirect this turn · Tab queues it for after", "escribe para redirigir este turno · …"), so it no longer names a command and the guard never matched: Stop sent nothing while the turn ran (seen live 2026-11-05). What every 0.22.5 locale shares is the input row's running-turn icon, `☤ ❯` (Hermes `cli_tui_mixin.py`, `_get_tui_prompt_fragments`: drawn only while the agent runs and no approval, clarify, sudo or command panel holds the input), and an empty input there is its placeholder, which the styled capture shows dim and italic. So the guard also takes that row when the input box reads empty. A gateway-backed Stop (hermes_gateway.rs) needs none of this; the guard is the fallback for a session no gateway answers.
/// SEE-ALSO: server/src/session_chat_send/steps.rs (`session_chat_interrupt_escape_steps`), server/src/session_chat_send/worker.rs (`GuardHermesInterrupt`).
/// `screen` is the styled (VT) capture, so the placeholder can be told from typed text.
pub(crate) fn hermes_turn_is_interruptible(screen: &str) -> bool {
    let lines: Vec<String> = screen
        .lines()
        .rev()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .filter(|line| !line.is_empty())
        .take(HERMES_BUSY_HINT_SCAN_LINES)
        .collect();
    if lines
        .iter()
        .any(|line| line.contains("/queue · /bg · /steer · Ctrl+C"))
    {
        return true;
    }
    screen
        .lines()
        .rev()
        .filter(|line| !strip_ansi_sgr(line).trim().is_empty())
        .take(HERMES_BUSY_HINT_SCAN_LINES)
        .any(running_row_with_empty_input)
}

/// `☤ ❯` followed by nothing, or by the placeholder, which Hermes 0.22.5 draws in the same italic
/// run as the icon (`\x1b[3m☤ ❯ type to redirect…`). Typed text follows a reset instead
/// (`\x1b[3m☤ ❯ \x1b[0mmy draft`), measured 2026-11-05 on 0.22.5.
fn running_row_with_empty_input(raw: &str) -> bool {
    const ROW: &str = "\u{2624} \u{276f}";
    let plain = normalize_spaces(&strip_ansi_sgr(raw));
    let Some(rest) = plain
        .trim()
        .trim_start_matches(['\u{2502}', '|'])
        .trim_start()
        .strip_prefix(ROW)
    else {
        return false;
    };
    if rest.trim().is_empty() {
        return true;
    }
    let Some(marker) = raw.find('\u{276f}') else {
        return false;
    };
    let before = &raw[..marker];
    let after = &raw[marker + '\u{276f}'.len_utf8()..];
    let italic = before.rfind("\x1b[3m").is_some_and(|start| {
        let tail = &before[start..];
        !tail.contains("\x1b[0m") && !tail.contains("\x1b[23m") && !tail.contains("\x1b[m")
    });
    // The first visible character after the marker must still be inside that italic run: skip
    // spaces and whole escape sequences, never stopping inside one.
    let bytes = after.as_bytes();
    let mut first_visible = 0;
    while first_visible < bytes.len() {
        if bytes[first_visible] == 0x1b && bytes.get(first_visible + 1) == Some(&b'[') {
            first_visible += 2;
            while first_visible < bytes.len() && !(0x40..=0x7e).contains(&bytes[first_visible]) {
                first_visible += 1;
            }
            first_visible += 1;
        } else if bytes[first_visible].is_ascii_whitespace() {
            first_visible += 1;
        } else {
            break;
        }
    }
    let first_visible = first_visible.min(after.len());
    let between = &after[..first_visible];
    italic
        && !between.contains("\x1b[0m")
        && !between.contains("\x1b[23m")
        && !between.contains("\x1b[m")
}

/// Records a Ctrl+C for this session, or refuses when the previous one is still inside Hermes's
/// force-quit window.
pub(crate) fn claim_hermes_interrupt(project_id: &str, session_id: &str) -> bool {
    static LAST_INTERRUPT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let Ok(mut last) = LAST_INTERRUPT.get_or_init(Mutex::default).lock() else {
        return false;
    };
    last.retain(|_, at| at.elapsed() < HERMES_INTERRUPT_SPACING);
    let key = crate::server::session_observer_key(project_id, session_id);
    if last.contains_key(&key) {
        return false;
    }
    last.insert(key, Instant::now());
    true
}
