use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::session_chat_send::{
    write_session_chat_payload, SessionChatSendError, SessionChatSendFailure,
    SESSION_CHAT_INTERRUPT,
};

/// Why an account switch's exit gave up: the agent kept the terminal past the time limit.
pub(crate) const AGENT_DID_NOT_EXIT: &str =
    "The agent did not exit back to the shell, so the resume command was not typed.";

/// Why an account switch's exit gave up: the session's processes could not be read, so whether the
/// shell had the terminal back was never known.
pub(crate) const AGENT_NOT_INSPECTED: &str =
    "Ghostex could not check the session's processes while exiting the agent, so the resume command was not typed.";

/// Consecutive failed reads of the session's processes the exit sits out before it gives up.
const FAILED_READS_LIMIT: u32 = 10;

/// Whether `error` is the exit giving up without the resume command typed (the agent kept the
/// terminal, or the session could not be read), the failures a Full Reload of the session on the
/// selected account gets past (accounts/restart.rs).
///
/// CDXC:AgentProviders 2026-11-16 WHY:
/// User, on GitHub #630: "Full reload is better than sleep wake". A switch that could not read the
/// session used to stop with "Could not inspect the agent", the agent already stopped and the
/// session holding an empty shell, so the user had to put it to sleep, switch and wake it. A failed
/// read now ends in the same recovery as an agent that will not exit: the session is reloaded on
/// the selected account and resumes its conversation there.
pub(crate) fn reload_recovers(error: &SessionChatSendError) -> bool {
    error.failure == SessionChatSendFailure::ComposerNotReady
        && (error.message == AGENT_DID_NOT_EXIT || error.message == AGENT_NOT_INSPECTED)
}

/// Account switching deliberately cancels the old agent regardless of its screen.
/// Each interrupt is a separate stdin write; consecutive Ctrl+C presses must land inside the CLI's double-interrupt window.
///
/// CDXC:AgentProviders 2026-09-29 WHY:
/// With a background shell or agent running, Claude's second Ctrl+C opens an "Exit and stop tasks / Move to background and exit / Stay" dialog that needs two more Ctrl+C presses to exit (stopping the tasks). The old three-Ctrl+C cycle sent its Escape right after the third press, cancelling that dialog every round, so the switch looped until it timed out. Each round therefore sends four Ctrl+C presses before its Escape.
pub(crate) async fn interrupt_until_exited(
    _home: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    timeout_ms: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), SessionChatSendError> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let keys = [SESSION_CHAT_INTERRUPT, "\u{3}", "\u{3}", "\u{3}", "\u{3}"];
    let mut next_key = 0;
    let mut shell_ready_since = None;
    let mut shell_starting_since = None;
    let mut failed_reads = 0;
    loop {
        if cancelled() {
            return Err(error(
                SessionChatSendFailure::NotAttempted,
                "The switch was cancelled.",
            ));
        }
        // CDXC:AgentProviders 2026-11-16 WHY: the first read runs before any key is sent, so a
        // session that cannot be read is never stopped by this loop (GitHub #630). A read that
        // fails once the agent is stopping is retried; only reads that keep failing give up, and
        // the switch then reloads the session (`reload_recovers`).
        let name = zmx_name.to_string();
        let observation = tokio::task::spawn_blocking(move || observe(&name)).await;
        let (running, owner) = match observation {
            Ok(Ok(observation)) => {
                failed_reads = 0;
                observation
            }
            _ => {
                failed_reads += 1;
                if failed_reads >= FAILED_READS_LIMIT || Instant::now() >= deadline {
                    return Err(error(
                        SessionChatSendFailure::ComposerNotReady,
                        AGENT_NOT_INSPECTED,
                    ));
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
                continue;
            }
        };
        if owner == TerminalOwner::ShellStarting {
            shell_starting_since.get_or_insert_with(Instant::now);
        } else {
            shell_starting_since = None;
        }
        let shell_ready = owner == TerminalOwner::ShellPrompt
            || shell_starting_since.is_some_and(|since| since.elapsed() >= SHELL_STARTING_LIMIT);
        if shell_ready {
            if shell_ready_since.get_or_insert_with(Instant::now).elapsed()
                >= Duration::from_millis(300)
            {
                // An interrupt can straddle the CLI exit and leave escape-sequence
                // bytes in the shell editor. Clear that line before the resume command.
                write_session_chat_payload(project_id, session_id, zmx_name, source, "\u{3}")
                    .await
                    .map_err(|message| error(SessionChatSendFailure::Write, &message))?;
                tokio::time::sleep(Duration::from_millis(150)).await;
                return Ok(());
            }
        } else {
            shell_ready_since = None;
        }
        if Instant::now() >= deadline {
            return Err(error(
                SessionChatSendFailure::ComposerNotReady,
                AGENT_DID_NOT_EXIT,
            ));
        }
        if cancelled() {
            return Err(error(
                SessionChatSendFailure::NotAttempted,
                "The switch was cancelled.",
            ));
        }
        if running {
            write_session_chat_payload(project_id, session_id, zmx_name, source, keys[next_key])
                .await
                .map_err(|message| error(SessionChatSendFailure::Write, &message))?;
            next_key = (next_key + 1) % keys.len();
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Who holds a session's terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalOwner {
    /// The session's shell, reading a line at its prompt (or a shell whose terminal mode could not
    /// be read, which is how every shell counted before the mode was read).
    ShellPrompt,
    /// The session's shell with its terminal in line mode: it is still running its startup files,
    /// where an agent session's start command has not run yet (zmx/zsh_startup.rs).
    ShellStarting,
    /// A command the shell started: an agent, its launcher, a resume lookup.
    Command,
}

/// How long a shell may hold its terminal in line mode before it counts as idle anyway: a shell
/// without a line editor reads its prompt that way.
const SHELL_STARTING_LIMIT: Duration = Duration::from_secs(8);

/// Who holds the session's terminal, from one read of the session's own processes
/// (zmx/session_tree.rs); `Err` when they cannot be read or no daemon runs for the session.
pub(crate) fn terminal_owner(zmx_name: &str) -> Result<TerminalOwner, ()> {
    observe(zmx_name).map(|(_, owner)| owner)
}

/// Whether the session runs an agent, and who holds its terminal.
///
/// CDXC:AgentProviders 2026-10-24 WHY:
/// A shell holding its terminal is not yet a shell at its prompt. Until its startup files finish, zsh holds the terminal in line mode and an agent session's start command is still to run from the first prompt (zmx/zsh_startup.rs), so a resume or switch command typed then sits in the terminal's input until that start command reads it. A draft switched from Claude to Codex in its first seconds could do that, or its command could be left unrun at the prompt (GitHub #290). The shell counts as at its prompt only once its line editor has taken the terminal out of line mode (zsh, bash and fish all do); the mode is read from the terminal itself.
fn is_acp_host_process(pid: i64, command: &str, root_pid: i64) -> bool {
    if pid == root_pid {
        return false;
    }
    let mut parts = command.split_whitespace();
    let Some(exe) = parts.next() else {
        return false;
    };
    let stem = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    matches!(
        stem,
        "ghostex" | "ghostex.exe" | "gxserver" | "gxserver.exe"
    ) && parts.next() == Some("acp-host")
}

fn observe(zmx_name: &str) -> Result<(bool, TerminalOwner), ()> {
    let tree = crate::zmx::read_zmx_session_tree(zmx_name)?.ok_or(())?;
    if tree.agent_identity().is_some()
        || tree
            .processes
            .iter()
            .any(|process| is_acp_host_process(process.pid, &process.command, tree.root))
    {
        return Ok((true, TerminalOwner::Command));
    }
    let Some(tty) = shell_owning_terminal(&tree) else {
        return Ok((false, TerminalOwner::Command));
    };
    Ok((
        false,
        match line_mode(&tty) {
            Some(true) => TerminalOwner::ShellStarting,
            Some(false) | None => TerminalOwner::ShellPrompt,
        },
    ))
}

/// Whether the terminal device `tty` (as `ps` prints it) is in line mode, `None` when unreadable.
#[cfg(not(windows))]
fn line_mode(tty: &str) -> Option<bool> {
    if tty.is_empty() || tty.starts_with('?') {
        return None;
    }
    let path = if tty.starts_with('/') {
        tty.to_string()
    } else {
        format!("/dev/{tty}")
    };
    let path = std::ffi::CString::new(path).ok()?;
    // SAFETY: a plain open/tcgetattr/close on a NUL-terminated path; the fd never escapes.
    unsafe {
        let fd = libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK,
        );
        if fd < 0 {
            return None;
        }
        let mut termios: libc::termios = std::mem::zeroed();
        let read = libc::tcgetattr(fd, &mut termios) == 0;
        libc::close(fd);
        read.then_some(termios.c_lflag & libc::ICANON != 0)
    }
}

#[cfg(windows)]
fn line_mode(_: &str) -> Option<bool> {
    None
}

/// CDXC:AgentProviders 2026-09-22 WHY:
/// A missing agent PID can be a gap inside an older shell restore script, with a resume lookup or account wrapper still running. Only an idle foreground shell may receive the new account's command.
/// Answers the shell's terminal device while the daemon's own shell holds the terminal alone.
#[cfg(not(windows))]
fn shell_owning_terminal(tree: &crate::zmx::SessionTree) -> Option<String> {
    let processes = &tree.processes;
    processes
        .iter()
        .find(|process| {
            process.ppid == tree.root
                && process.group > 0
                && process.group == process.foreground_group
                && process
                    .command
                    .split_whitespace()
                    .next()
                    .and_then(|name| Path::new(name).file_name())
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        matches!(
                            name.trim_start_matches('-'),
                            "sh" | "bash" | "zsh" | "fish" | "ksh" | "dash" | "nu"
                        )
                    })
                && !processes
                    .iter()
                    .any(|other| other.pid != process.pid && other.group == process.group)
        })
        .map(|process| process.terminal.clone().unwrap_or_default())
}

// Native Windows has no POSIX foreground process groups. It retains the agent-process exit check above.
#[cfg(windows)]
fn shell_owning_terminal(_: &crate::zmx::SessionTree) -> Option<String> {
    Some(String::new())
}

fn error(failure: SessionChatSendFailure, message: &str) -> SessionChatSendError {
    SessionChatSendError {
        failure,
        message: message.to_string(),
    }
}
