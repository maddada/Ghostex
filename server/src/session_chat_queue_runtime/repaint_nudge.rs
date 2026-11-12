//! The input-box ladder's resize step: an idle agent whose input box stays unreadable after the
//! fresh reads and Claude's Ctrl+L gets its terminal made one row shorter and put back, so it
//! repaints its whole screen the way opening the session's terminal view did.
//!
//! CDXC:SessionChat 2026-10-29 WHY:
//! What freed the stuck coordinator (S90:P3lv0:G1pif) was the terminal view's resize: the agent got
//! SIGWINCH with a new size and repainted every cell. A bare SIGWINCH is not enough, because Node
//! (Claude Code) emits `resize` only when the size really changed, and attaching another zmx client
//! to force a resize could take leadership and resize the user's own terminal. So the step sets the
//! session pty's size directly (TIOCSWINSZ, as `stty rows` does), one row smaller and back; each
//! change makes the kernel signal the agent's foreground process group. It types nothing, so it is
//! safe for every agent, and it runs only on the same guards as the Ctrl+L (held_redraw.rs): a
//! running session idle by its hooks, no notice, dialog or compaction on screen, the input box
//! unreadable, the agent not starting. First after 45 s of an unreadable box (the Ctrl+L at 20 s
//! had its chance), then at most every two minutes, each logged with its screen
//! (`sessionChatQueueResizedForRepaint` in session-chat-send-failures.jsonl). The size is put back
//! only if nobody resized the terminal in between. On native Windows wmx resizes the session's
//! console (`resize_and_restore` there).
//! SEE-ALSO: wait_reason.rs (the ladder's thresholds), wiring.rs (runs this after each fresh read).

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use crate::server::AppState;
use crate::session_chat_options::SessionChatTerminalDetection;

const FIRST_NUDGE_AFTER: Duration = Duration::from_secs(45);
const NUDGE_EVERY: Duration = Duration::from_secs(120);

#[derive(Clone, Copy)]
struct Clock {
    unreadable_since: Instant,
    last_nudge: Option<Instant>,
}

static CLOCKS: LazyLock<Mutex<HashMap<String, Clock>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether this fresh screen is one a repaint may fix: an agent Ghostex can read the input box of,
/// that box missing, nothing else (a notice, a panel Escape closes, a compaction or turn) on
/// screen, and the agent not still starting.
pub(crate) fn screen_wants_repaint(
    agent: Option<&str>,
    detection: &SessionChatTerminalDetection,
) -> bool {
    let composer = &detection.composer;
    crate::session_chat_composer::has_session_chat_composer_signature(agent)
        && detection.notice.is_none()
        && (detection.activity.is_none()
            || crate::session_chat_terminal_activity::is_session_chat_background_work_activity(
                detection.activity.as_ref(),
            ))
        && composer.is_not_ready()
        && composer.input_box_missing()
        && !composer.should_dismiss()
        && !composer.screen_tail.is_empty()
        && !crate::session_chat_send_wake::screen_shows_agent_starting(&composer.screen_tail)
}

pub(crate) fn nudge_due(unreadable_since: Instant, last: Option<Instant>, now: Instant) -> bool {
    now.duration_since(unreadable_since) >= FIRST_NUDGE_AFTER
        && last.is_none_or(|last| now.duration_since(last) >= NUDGE_EVERY)
}

/// Runs after the scheduler's fresh screen read of a session holding a message. Blocking.
pub(crate) fn nudge_held_repaint_if_due(
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
    if !screen_wants_repaint(agent, detection) {
        clocks.remove(&key);
        return;
    }
    let clock = *clocks.entry(key.clone()).or_insert(Clock {
        unreadable_since: now,
        last_nudge: None,
    });
    if !nudge_due(clock.unreadable_since, clock.last_nudge, now) {
        return;
    }
    let Some(session) = super::held_redraw::read_session(state, project_id, session_id) else {
        return;
    };
    if !super::held_redraw::session_is_idle(&session) {
        return;
    }
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    clocks.insert(
        key,
        Clock {
            last_nudge: Some(now),
            ..clock
        },
    );
    drop(clocks);
    let outcome = resize_and_restore(&zmx_name);
    let reason = match &outcome {
        Ok((rows, cols)) => format!(
            "{} Resized {rows}x{cols} to {}x{cols} and back.",
            detection.composer.reason.as_deref().unwrap_or_default(),
            rows.saturating_sub(1)
        ),
        Err(error) => format!(
            "{} The resize could not be done: {error}",
            detection.composer.reason.as_deref().unwrap_or_default()
        ),
    };
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionChatQueueResizedForRepaint",
        project_id,
        session_id,
        &reason,
        &detection.composer.screen_tail,
    );
}

/// The pty the session's agent runs on: the terminal of the zmx daemon's child process.
#[cfg(unix)]
fn session_tty(zmx_name: &str) -> Result<std::path::PathBuf, String> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,tty=,command="])
        .output()
        .map_err(|error| error.to_string())?;
    let table = String::from_utf8_lossy(&output.stdout);
    let daemon = crate::zmx::find_zmx_daemon_process_id(&table, zmx_name)
        .ok_or_else(|| "the session's zmx daemon is not running".to_string())?;
    child_tty(&table, daemon).ok_or_else(|| "the session has no terminal".to_string())
}

/// The terminal of the first child of `parent` in a `ps -o pid=,ppid=,tty=,…` table.
#[cfg(unix)]
fn child_tty(table: &str, parent: i64) -> Option<std::path::PathBuf> {
    table.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        let _pid = columns.next()?.parse::<i64>().ok()?;
        let ppid = columns.next()?.parse::<i64>().ok()?;
        let tty = columns.next()?;
        (ppid == parent && tty != "??" && tty != "?")
            .then(|| std::path::Path::new("/dev").join(tty))
    })
}

/// Makes the pty one row shorter and puts it back; answers the size it had.
#[cfg(unix)]
fn resize_and_restore(zmx_name: &str) -> Result<(u16, u16), String> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let tty = session_tty(zmx_name)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(&tty)
        .map_err(|error| format!("{}: {error}", tty.display()))?;
    let fd = file.as_raw_fd();
    let read_size = || {
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        (unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) } == 0).then_some(size)
    };
    let set_size = |size: &libc::winsize| unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, size) } == 0;
    let original = read_size().ok_or("the terminal size could not be read")?;
    if original.ws_row < 3 {
        return Err(format!(
            "the terminal is only {} rows tall",
            original.ws_row
        ));
    }
    let smaller = libc::winsize {
        ws_row: original.ws_row - 1,
        ..original
    };
    if !set_size(&smaller) {
        return Err("the terminal could not be resized".to_string());
    }
    // Long enough for the agent to take the first SIGWINCH as its own resize.
    std::thread::sleep(Duration::from_millis(250));
    let current = read_size();
    if current.is_some_and(|size| size.ws_row == smaller.ws_row && size.ws_col == smaller.ws_col)
        && !set_size(&original)
    {
        return Err("the terminal size could not be put back".to_string());
    }
    Ok((original.ws_row, original.ws_col))
}

/// CDXC:SessionChat 2026-11-12 WHY:
/// On native Windows this step only logged "The resize could not be done: not available on this platform", so a coordinator whose Claude input box could not be read (S8g, 2026-11-12 07:13 to 07:17 UTC) had no repaint at all until the reload at four minutes. wmx owns the session's console, and its `resize` request without a client id sets the console size directly (ConPTY), which Claude Code's Node sees as a terminal resize, as the pty resize does on macOS and Linux. The size comes from wmx's `grid` and is put back only if it is still the smaller one.
#[cfg(windows)]
fn resize_and_restore(zmx_name: &str) -> Result<(u16, u16), String> {
    let program = crate::toolchain::require_bundled_zmx()
        .map_err(|error| error.to_string())?
        .executable_path;
    let size = || -> Result<(u16, u16), String> {
        let grid = crate::zmx::screen_capture::read_zmx_grid(zmx_name)
            .ok_or("the session's console size could not be read")?;
        wmx_grid_size(&grid).ok_or_else(|| "wmx answered no console size".to_string())
    };
    let resize = |rows: u16, cols: u16| -> Result<(), String> {
        let result = crate::zmx::run_zmx_interaction_command(
            crate::zmx::build_zmx_resize_command(zmx_name, &program, rows, cols),
            crate::zmx::ZmxCommandOptions::default(),
        )
        .map_err(|error| match error {
            crate::zmx::ZmxEndpointError::DependencyUnavailable(message) => message,
            crate::zmx::ZmxEndpointError::Domain(error) => error.message,
        })?;
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(format!(
                "wmx resize exited {}: {}",
                result.exit_code,
                result.stderr.trim()
            ))
        }
    };
    let (rows, cols) = size()?;
    if rows < 3 {
        return Err(format!("the console is only {rows} rows tall"));
    }
    resize(rows - 1, cols)?;
    // Long enough for the agent to take the first change as its own resize.
    std::thread::sleep(Duration::from_millis(250));
    if size().is_ok_and(|now| now == (rows - 1, cols)) {
        resize(rows, cols).map_err(|error| format!("the size could not be put back: {error}"))?;
    }
    Ok((rows, cols))
}

/// The console size in wmx's `grid` answer (`display.rs` `snapshot`).
#[cfg(any(windows, test))]
fn wmx_grid_size(grid_json: &str) -> Option<(u16, u16)> {
    let grid: serde_json::Value = serde_json::from_str(grid_json.trim()).ok()?;
    let rows = u16::try_from(grid.get("rows")?.as_u64()?).ok()?;
    let cols = u16::try_from(grid.get("cols")?.as_u64()?).ok()?;
    Some((rows, cols))
}

#[cfg(not(any(unix, windows)))]
fn resize_and_restore(_zmx_name: &str) -> Result<(u16, u16), String> {
    Err("not available on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat_composer::detect_session_chat_composer_readiness;

    fn detection(agent: &str, screen: &str) -> SessionChatTerminalDetection {
        SessionChatTerminalDetection {
            composer: detect_session_chat_composer_readiness(Some(agent), screen, None),
            ..SessionChatTerminalDetection::default()
        }
    }

    #[test]
    fn an_unreadable_box_is_repainted_for_claude_and_codex_but_not_a_starting_agent() {
        let claude = "\u{273b} Cooked for 9s\n\n\u{276f} \n\n          48\n  \u{23f5}\u{23f5} auto mode on (shift+tab to cycle)\n";
        assert!(screen_wants_repaint(
            Some("claude"),
            &detection("claude", claude)
        ));
        let codex = "\u{2022} Done.\n\n          48\n";
        let seen = detection("codex", codex);
        assert!(seen.composer.input_box_missing(), "{:?}", seen.composer);
        assert!(screen_wants_repaint(Some("codex"), &seen));
        assert!(!screen_wants_repaint(
            Some("claude"),
            &detection("claude", "Restoring session...\n")
        ));
        // An agent Ghostex cannot read the input box of is never nudged.
        assert!(!screen_wants_repaint(
            Some("unknown-agent"),
            &detection("unknown-agent", claude)
        ));
    }

    #[test]
    fn the_console_size_is_read_from_wmx_grid() {
        let grid = r#"{"rows":24,"cols":200,"leader_fd":-1,"resting_rows":50,"resting_cols":200,"chat_claim":true,"clients":[]}"#;
        assert_eq!(wmx_grid_size(grid), Some((24, 200)));
        assert_eq!(wmx_grid_size("{}"), None);
        assert_eq!(wmx_grid_size("not json"), None);
    }

    #[test]
    fn a_readable_box_is_left_alone() {
        let rule = "\u{2500}".repeat(60);
        let whole = format!("\u{23fa} Done.\n{rule}\n\u{276f} \n{rule}\n");
        assert!(!screen_wants_repaint(
            Some("claude"),
            &detection("claude", &whole)
        ));
    }

    #[test]
    fn the_nudge_waits_45_seconds_then_comes_every_two_minutes() {
        let since = Instant::now();
        assert!(!nudge_due(since, None, since + Duration::from_secs(44)));
        assert!(nudge_due(since, None, since + Duration::from_secs(45)));
        let last = since + Duration::from_secs(45);
        assert!(!nudge_due(
            since,
            Some(last),
            last + Duration::from_secs(119)
        ));
        assert!(nudge_due(
            since,
            Some(last),
            last + Duration::from_secs(120)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn the_agent_tty_is_the_daemon_child_terminal() {
        let table = "  100     1 ??       zmx run S1-P1-G1 -d\n  101   100 ttys006  /bin/zsh\n  102   101 ttys006  claude\n";
        assert_eq!(
            child_tty(table, 100),
            Some(std::path::PathBuf::from("/dev/ttys006"))
        );
        assert_eq!(child_tty(table, 7), None);
        let linux = "  100     1 ?        zmx\n  101   100 pts/3    bash\n";
        assert_eq!(
            child_tty(linux, 100),
            Some(std::path::PathBuf::from("/dev/pts/3"))
        );
    }

    /// GitHub #400: Linux procps writes `?` where macOS writes `??`, and the daemon was never found.
    #[cfg(unix)]
    #[test]
    fn the_session_daemon_is_found_on_linux_and_macos() {
        let macos = "  100     1 ??       /Applications/Ghostex.app/Contents/Resources/bin/zmx run S1x-P1abc-G1abc -d\n  101   100 ttys006  /bin/zsh\n";
        let linux = "    100       1 ?        /opt/ghostex/gxserver/bin/zmx run S1x-P1abc-G1abc -d\n    101     100 pts/3    bash\n    102     101 pts/3    claude\n";
        for table in [macos, linux] {
            assert_eq!(
                crate::zmx::find_zmx_daemon_process_id(table, "S1x-P1abc-G1abc"),
                Some(100),
                "{table}"
            );
            assert_eq!(
                crate::zmx::find_zmx_daemon_process_id(table, "S1x-P1abc-G2abc"),
                None
            );
        }
        assert_eq!(
            child_tty(linux, 100),
            Some(std::path::PathBuf::from("/dev/pts/3"))
        );
    }
}
