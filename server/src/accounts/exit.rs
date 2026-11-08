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

/// Whether `error` is the exit giving up with the agent still holding the terminal, the one failure a
/// stop-and-restart of the session gets past (accounts/restart.rs).
pub(crate) fn did_not_exit(error: &SessionChatSendError) -> bool {
    error.failure == SessionChatSendFailure::ComposerNotReady && error.message == AGENT_DID_NOT_EXIT
}

/// Account switching deliberately cancels the old agent regardless of its screen.
/// Each interrupt is a separate stdin write; consecutive Ctrl+C presses must land inside the CLI's double-interrupt window.
///
/// CDXC:AgentProviders 2026-09-29 WHY:
/// With a background shell or agent running, Claude's second Ctrl+C opens an "Exit and stop tasks / Move to background and exit / Stay" dialog that needs two more Ctrl+C presses to exit (stopping the tasks). The old three-Ctrl+C cycle sent its Escape right after the third press, cancelling that dialog every round, so the switch looped until it timed out. Each round therefore sends four Ctrl+C presses before its Escape.
pub(crate) async fn interrupt_until_exited(
    home: &Path,
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
    loop {
        if cancelled() {
            return Err(error(
                SessionChatSendFailure::NotAttempted,
                "The switch was cancelled.",
            ));
        }
        let name = zmx_name.to_string();
        let home = home.to_path_buf();
        let running = tokio::task::spawn_blocking(move || {
            crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(&name), &home)
                .map_err(|_| ())
                .and_then(|identities| {
                    let running = identities.contains_key(&name);
                    let owner = if running {
                        TerminalOwner::Command
                    } else {
                        terminal_owner(&name)?
                    };
                    Ok((running, owner))
                })
        })
        .await;
        let (running, owner) = match running {
            Ok(Ok(observation)) => observation,
            _ => {
                return Err(error(
                    SessionChatSendFailure::ComposerNotReady,
                    "Could not inspect the agent while exiting it. Retry the switch.",
                ))
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

/// CDXC:AgentProviders 2026-10-24 WHY:
/// A shell holding its terminal is not yet a shell at its prompt. Until its startup files finish, zsh holds the terminal in line mode and an agent session's start command is still to run from the first prompt (zmx/zsh_startup.rs), so a resume or switch command typed then sits in the terminal's input until that start command reads it. A draft switched from Claude to Codex in its first seconds could do that, or its command could be left unrun at the prompt (GitHub #290). The shell counts as at its prompt only once its line editor has taken the terminal out of line mode (zsh, bash and fish all do); the mode is read from the terminal itself.
pub(crate) fn terminal_owner(zmx_name: &str) -> Result<TerminalOwner, ()> {
    let Some(tty) = shell_owning_terminal(zmx_name)? else {
        return Ok(TerminalOwner::Command);
    };
    Ok(match line_mode(&tty) {
        Some(true) => TerminalOwner::ShellStarting,
        Some(false) | None => TerminalOwner::ShellPrompt,
    })
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
/// The process capture must drain stdout while ps runs; waiting for exit before reading fills the pipe on machines with many sessions and falsely times out.
/// Answers the shell's terminal device while the shell holds the terminal.
#[cfg(not(windows))]
fn shell_owning_terminal(zmx_name: &str) -> Result<Option<String>, ()> {
    let output = crate::zmx::run_zmx_probe_script(
        "ps -axo pid=,ppid=,pgid=,tpgid=,tty=,command=".to_string(),
        crate::zmx::ZmxCommandOptions {
            stdout_limit_bytes: Some(crate::zmx::GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES),
            timeout_ms: Some(3_000),
            ..Default::default()
        },
    )
    .map_err(|_| ())?;
    if output.exit_code != 0 || output.stdout_truncated {
        return Err(());
    }
    let mut rows = Vec::new();
    let mut identity_rows = String::new();
    for line in output.stdout.lines() {
        let mut fields = line.split_whitespace();
        let mut number = || fields.next().and_then(|value| value.parse::<i64>().ok());
        let (Some(pid), Some(ppid), Some(pgid), Some(tpgid)) =
            (number(), number(), number(), number())
        else {
            return Err(());
        };
        let tty = fields.next().ok_or(())?.to_string();
        let command = fields.collect::<Vec<_>>().join(" ");
        identity_rows.push_str(&format!("{pid} {ppid} {command}\n"));
        rows.push((pid, ppid, pgid, tpgid, tty, command));
    }
    let root = crate::zmx::find_zmx_daemon_process_id(&identity_rows, zmx_name).ok_or(())?;
    let shell = rows.iter().find(|(pid, ppid, pgid, tpgid, _, command)| {
        *ppid == root
            && *pgid > 0
            && pgid == tpgid
            && command
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
            && !rows
                .iter()
                .any(|(other_pid, _, other_group, _, _, _)| other_pid != pid && other_group == pgid)
    });
    Ok(shell.map(|(_, _, _, _, tty, _)| tty.clone()))
}

// Native Windows has no POSIX foreground process groups. It retains the agent-process exit check above.
#[cfg(windows)]
fn shell_owning_terminal(_: &str) -> Result<Option<String>, ()> {
    Ok(Some(String::new()))
}

fn error(failure: SessionChatSendFailure, message: &str) -> SessionChatSendError {
    SessionChatSendError {
        failure,
        message: message.to_string(),
    }
}
