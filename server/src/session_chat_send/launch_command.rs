use super::*;

use crate::accounts::exit::TerminalOwner;

/// How often a typed start command is checked until it runs.
const LAUNCH_COMMAND_POLL_MS: u64 = 250;
/// How long the screen must keep showing the same state before it is acted on, so a repaint in
/// flight is never mistaken for a lost Enter.
const LAUNCH_COMMAND_STABLE_MS: u64 = 1_000;
/// How much of the command's end is matched on screen. Agent start lines run to kilobytes and
/// scroll their beginning off a small screen.
const LAUNCH_LINE_MATCH_CHARS: usize = 160;
/// How many stray keys a terminal may echo after the typed command (`^G^G`) while it still counts
/// as unrun.
const LAUNCH_LINE_ECHOED_KEYS: usize = 8;

/// What the screen shows of a start command typed at a shell prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaunchLine {
    /// The command is the last thing on screen: it sits at the prompt, unrun.
    Waiting,
    /// A bare shell prompt and none of the command: the keystrokes were lost.
    Lost,
    /// Anything else: it ran (and printed, or ended), or the screen shows something unrelated.
    Other,
}

/// How often the command's start and end were on screen before it was typed, so an earlier copy
/// (the same switch made before, still on screen) is not taken for this one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShownBefore {
    starts: usize,
    ends: usize,
}

/// The command and the screen without whitespace, with the command's first and last
/// `LAUNCH_LINE_MATCH_CHARS` characters.
struct LaunchText {
    typed: String,
    start: String,
    end: String,
    on_screen: String,
}

impl LaunchText {
    fn new(screen: &str, command: &str) -> Option<Self> {
        let compact = |text: &str| {
            text.chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>()
        };
        let typed = compact(command);
        if typed.is_empty() {
            return None;
        }
        let skip = typed
            .chars()
            .count()
            .saturating_sub(LAUNCH_LINE_MATCH_CHARS);
        Some(Self {
            start: typed.chars().take(LAUNCH_LINE_MATCH_CHARS).collect(),
            end: typed.chars().skip(skip).collect(),
            on_screen: compact(screen),
            typed,
        })
    }

    fn shown(&self) -> ShownBefore {
        ShownBefore {
            starts: self.on_screen.matches(&self.start).count(),
            ends: self.on_screen.matches(&self.end).count(),
        }
    }
}

/// What `screen` showed of `command` before it was typed.
pub(crate) fn shown_before(screen: &str, command: &str) -> ShownBefore {
    LaunchText::new(screen, command).map_or_else(ShownBefore::default, |text| text.shown())
}

/// CDXC:SessionChat 2026-10-24 WHY:
/// The test for a start command left at the shell prompt (GitHub #290: a new chat switched to Codex showed `codex` typed but not run, and the keys that followed landed in the shell). Whitespace is ignored because a long line wraps across screen rows. An agent's start line runs to kilobytes, so the screen may show only part of it: zsh's line editor draws a line taller than the screen from its start and leaves the rest undrawn (seen live with a 2.6 KB Codex line), while a terminal echoing typeahead scrolls the start away. So the command waits when the screen shows its start followed by nothing but more of it, or its end followed by nothing but a few keys echoed in caret notation. Only a copy the screen did not show before the command was typed counts: a switch made twice left the first copy on screen, which read as this one having run.
pub(crate) fn launch_line(screen: &str, command: &str, before: ShownBefore) -> LaunchLine {
    let Some(text) = LaunchText::new(screen, command) else {
        return LaunchLine::Other;
    };
    let shown = text.shown();
    if shown.starts > before.starts {
        let at = text.on_screen.rfind(&text.start).unwrap_or_default();
        let drawn = &text.on_screen[at..];
        return match drawn.strip_prefix(text.typed.as_str()) {
            Some(after) if only_echoed_keys(after) => LaunchLine::Waiting,
            Some(_) => LaunchLine::Other,
            None if text.typed.starts_with(drawn) => LaunchLine::Waiting,
            None => LaunchLine::Other,
        };
    }
    if shown.ends > before.ends {
        let at = text.on_screen.rfind(&text.end).unwrap_or_default();
        return if only_echoed_keys(&text.on_screen[at + text.end.len()..]) {
            LaunchLine::Waiting
        } else {
            LaunchLine::Other
        };
    }
    if crate::session_chat_notice::session_chat_screen_ends_with_shell_prompt(screen) {
        LaunchLine::Lost
    } else {
        LaunchLine::Other
    }
}

/// The live screen without scrollback, where an earlier copy of the command is least likely.
async fn grid_text(zmx_name: &str) -> Option<String> {
    let name = zmx_name.to_string();
    let capture =
        tokio::task::spawn_blocking(move || crate::zmx::read_zmx_session_grid_capture(&name))
            .await
            .ok()?
            .ok()?;
    (!capture.truncated).then_some(capture.text)
}

/// Whether `text` is nothing but keys a terminal echoes in caret notation (`^G`), never output.
fn only_echoed_keys(text: &str) -> bool {
    let mut characters = text.chars();
    let mut keys = 0;
    while let Some(character) = characters.next() {
        if character != '^' || characters.next().is_none() {
            return false;
        }
        keys += 1;
    }
    keys <= LAUNCH_LINE_ECHOED_KEYS
}

async fn type_command(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    command: &str,
) -> Result<(), SessionChatSendError> {
    let write = |text: String| async move {
        write_session_chat_payload(project_id, session_id, zmx_name, source, &text)
            .await
            .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))
    };
    write(command.to_string()).await?;
    tokio::time::sleep(Duration::from_millis(SESSION_CHAT_SUBMIT_DELAY_MS)).await;
    write(SESSION_CHAT_SUBMIT.to_string()).await
}

/// CDXC:SessionChat 2026-10-24 WHY:
/// A start command typed at a shell prompt (a draft's agent switch, an account switch) used to be typed and submitted blind, so one whose Enter was lost stayed at the prompt for good while the chat waited on an agent that never started, and the model pick's and draft handoff's keys landed in the shell (GitHub #290). After the Enter, this waits until the command holds the terminal or the agent runs. If the shell is back at its line editor with the command still the last thing on screen, it presses Enter once; if the prompt is bare and the command is gone, it types it once more. A shell still in line mode (loading its startup files) already holds the typed line and is left alone, because a second Enter would reach the agent that line starts. A command that still sits unrun after that fails the sequence with the reason.
/// Native Windows has no foreground process group to read, so there the command is typed and submitted as before.
/// SEE-ALSO: accounts/exit.rs (`terminal_owner`), agents/drafts.rs and accounts/restart.rs (the callers).
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_shell_command(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    command: &str,
    _home_dir: &Path,
    timeout_ms: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), SessionChatSendError> {
    if cfg!(windows) {
        return type_command(project_id, session_id, zmx_name, source, command).await;
    }
    let mut before = grid_text(zmx_name)
        .await
        .map_or_else(ShownBefore::default, |screen| {
            shown_before(&screen, command)
        });
    type_command(project_id, session_id, zmx_name, source, command).await?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut pressed_enter = false;
    let mut retyped = false;
    let mut seen: Option<(LaunchLine, Instant)> = None;
    loop {
        tokio::time::sleep(Duration::from_millis(LAUNCH_COMMAND_POLL_MS)).await;
        if cancelled() {
            return Err(SessionChatSendError::not_attempted(
                SESSION_CHAT_SEND_CANCELLED.to_string(),
            ));
        }
        let name = zmx_name.to_string();
        let owner =
            tokio::task::spawn_blocking(move || crate::accounts::exit::terminal_owner(&name))
                .await
                .unwrap_or(Err(()));
        let line = match owner {
            Ok(TerminalOwner::Command) => return Ok(()),
            Ok(TerminalOwner::ShellPrompt) => grid_text(zmx_name)
                .await
                .map_or(LaunchLine::Other, |screen| {
                    launch_line(&screen, command, before)
                }),
            // Still starting, or unreadable: nothing is decided from it.
            Ok(TerminalOwner::ShellStarting) | Err(()) => {
                seen = None;
                if Instant::now() >= deadline {
                    return Ok(());
                }
                continue;
            }
        };
        let since = match seen {
            Some((previous, since)) if previous == line => since,
            _ => {
                seen = Some((line, Instant::now()));
                Instant::now()
            }
        };
        let stable = since.elapsed() >= Duration::from_millis(LAUNCH_COMMAND_STABLE_MS);
        match line {
            // It ran and is already back at the prompt; whatever it printed is the start
            // failure card's to show.
            LaunchLine::Other if stable => return Ok(()),
            LaunchLine::Waiting if stable && !pressed_enter => {
                log_launch_recovery(
                    project_id,
                    session_id,
                    zmx_name,
                    source,
                    "sessionChatLaunchCommandPressedEnterAgain",
                    "The start command was still at the shell prompt, so Enter was pressed again.",
                );
                write_session_chat_payload(
                    project_id,
                    session_id,
                    zmx_name,
                    source,
                    SESSION_CHAT_SUBMIT,
                )
                .await
                .map_err(|message| {
                    SessionChatSendError::new(SessionChatSendFailure::Write, message)
                })?;
                pressed_enter = true;
                seen = None;
            }
            LaunchLine::Lost if stable && !retyped => {
                log_launch_recovery(
                    project_id,
                    session_id,
                    zmx_name,
                    source,
                    "sessionChatLaunchCommandRetyped",
                    "The start command was not on the shell prompt, so it was typed again.",
                );
                if let Some(screen) = grid_text(zmx_name).await {
                    before = shown_before(&screen, command);
                }
                type_command(project_id, session_id, zmx_name, source, command).await?;
                retyped = true;
                seen = None;
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            if matches!(line, LaunchLine::Waiting | LaunchLine::Lost) {
                log_launch_recovery(
                    project_id,
                    session_id,
                    zmx_name,
                    source,
                    "sessionChatLaunchCommandNotRun",
                    SESSION_CHAT_LAUNCH_COMMAND_NOT_RUN,
                );
                return Err(SessionChatSendError::new(
                    SessionChatSendFailure::ComposerNotReady,
                    SESSION_CHAT_LAUNCH_COMMAND_NOT_RUN.to_string(),
                ));
            }
            return Ok(());
        }
    }
}

const SESSION_CHAT_LAUNCH_COMMAND_NOT_RUN: &str =
    "The agent's start command stayed at the shell prompt without running, so the agent was not started.";

fn log_launch_recovery(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    event: &str,
    message: &str,
) {
    log_session_chat_paste_verification(
        LogLevel::Warn,
        event,
        project_id,
        session_id,
        zmx_name,
        source,
        0,
        0,
        message,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: &str = " codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low";

    #[test]
    fn a_command_left_at_the_prompt_is_waiting() {
        let screen = "Last login: Fri Oct 24 12:59:01\n~/dev/app ❯  codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low\n";
        assert_eq!(
            launch_line(screen, COMMAND, ShownBefore::default()),
            LaunchLine::Waiting
        );
    }

    #[test]
    fn keys_echoed_after_the_unrun_command_still_read_as_waiting() {
        // GitHub #290's screen: the draft handoff's Ctrl+G landed in the shell after the command.
        let screen = "~/dev/app ❯  codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low^G^G\n";
        assert_eq!(
            launch_line(screen, COMMAND, ShownBefore::default()),
            LaunchLine::Waiting
        );
    }

    #[test]
    fn a_long_command_wrapped_across_rows_is_matched() {
        let long = format!(
            " codex --no-daemon -c 'developer_instructions=\"{}\"'",
            "x".repeat(600)
        );
        let typed = format!("~/dev/app ❯ {}", long.trim());
        let rows: Vec<String> = typed
            .chars()
            .collect::<Vec<_>>()
            .chunks(200)
            .map(|row| row.iter().collect())
            .collect();
        assert_eq!(
            launch_line(&rows.join("\n"), &long, ShownBefore::default()),
            LaunchLine::Waiting
        );
    }

    #[test]
    fn a_command_taller_than_the_screen_shows_only_its_start() {
        let long = format!(
            " codex --no-daemon -c 'developer_instructions=\"{}\"' -c 'mcp_servers.x.tool_timeout_sec=1900'",
            "y".repeat(3_000)
        );
        let drawn: String = format!("~/dev/app ❯ {}", long.trim())
            .chars()
            .take(2_400)
            .collect();
        assert_eq!(
            launch_line(&drawn, &long, ShownBefore::default()),
            LaunchLine::Waiting
        );
        let ran = format!("{drawn}\n>_ OpenAI Codex (v0.164.0)");
        assert_eq!(
            launch_line(&ran, &long, ShownBefore::default()),
            LaunchLine::Other
        );
    }

    #[test]
    fn a_command_that_ran_is_not_waiting() {
        let started = "~/dev/app ❯  codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low\n>_ OpenAI Codex (v0.164.0)\n\n› Ask Codex to do anything\n\n  GPT-6.1-Sol low · ~/dev/app\n";
        assert_eq!(
            launch_line(started, COMMAND, ShownBefore::default()),
            LaunchLine::Other
        );
        let failed = "~/dev/app ❯  codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low\nzsh: command not found: codex\n~/dev/app ❯\n";
        assert_eq!(
            launch_line(failed, COMMAND, ShownBefore::default()),
            LaunchLine::Other
        );
    }

    #[test]
    fn a_bare_prompt_without_the_command_lost_it() {
        assert_eq!(
            launch_line(
                "Last login: Fri Oct 24\n~/dev/app ❯\n",
                COMMAND,
                ShownBefore::default()
            ),
            LaunchLine::Lost
        );
        assert_eq!(
            launch_line("user@host:~/dev/app$ \n", COMMAND, ShownBefore::default()),
            LaunchLine::Lost
        );
    }

    #[test]
    fn an_agent_screen_without_the_command_is_neither() {
        let codex = ">_ OpenAI Codex (v0.164.0)\n\n› Ask Codex to do anything\n\n  GPT-6.1-Sol low · ~/dev/app\n  ? for shortcuts\n";
        assert_eq!(
            launch_line(codex, COMMAND, ShownBefore::default()),
            LaunchLine::Other
        );
        assert_eq!(
            launch_line("", COMMAND, ShownBefore::default()),
            LaunchLine::Other
        );
    }

    #[test]
    fn an_earlier_copy_on_screen_is_not_this_command() {
        let earlier = "~/dev/app ❯  codex --no-daemon --model gpt-6.1-sol -c model_reasoning_effort=low\n>_ OpenAI Codex (v0.164.0)\n~/dev/app ❯\n";
        let before = shown_before(earlier, COMMAND);
        // The new copy's keystrokes were lost: only the old copy and a bare prompt are drawn.
        let lost = format!("{earlier}~/dev/app ❯\n");
        assert_eq!(launch_line(&lost, COMMAND, before), LaunchLine::Lost);
        let waiting = format!("{earlier}~/dev/app ❯  {}\n", COMMAND.trim());
        assert_eq!(launch_line(&waiting, COMMAND, before), LaunchLine::Waiting);
    }
}
