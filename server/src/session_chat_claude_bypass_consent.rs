//! Claude Code's Bypass Permissions warning, answered on screen for a session that started in
//! bypass.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::session_chat_notice::SessionChatTerminalNotice;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::capture_session_terminal_text;

const TITLE: &str = "WARNING: Claude Code running in Bypass Permissions mode";
const BODY: &str = "In Bypass Permissions mode, Claude Code will not ask for your approval before running potentially dangerous commands. This mode should only be used in a sandboxed container/VM that has restricted internet access and can easily be restored if damaged. By proceeding, you accept all responsibility for actions taken while running in Bypass Permissions mode. https://code.claude.com/docs/en/security";
const DECLINE: &str = "No, exit";
const ACCEPT: &str = "Yes, I accept";
const FOOTER: &str = "Enter to confirm · Esc to cancel";
/// The warning must read the same across this gap before a key is sent, as for a folder-trust
/// prompt (session_chat_trust_memory.rs): Claude may paint it before it reads input.
const SETTLE: Duration = Duration::from_millis(700);
/// How long one answer is given to close the warning before the next try.
const CLEAR_WAIT: Duration = Duration::from_millis(2000);
const ATTEMPTS: usize = 3;
/// A session whose answers all failed is left to its card for this long.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/*
CDXC:AgentProviders 2026-10-31 DECISION:
User: "please when u see this warning just accept it automatically for the user" (Claude Code's Bypass Permissions warning). Launches prevent it with Claude's own consent setting (agents/bypass_consent.rs); this answers it when it paints anyway (a user command that names its own `--settings`, a Claude that ignores the setting), and only for a session whose launch command starts in bypass, so it never puts a session into bypass. The match is the whole dialog, word for word: its title, its text, exactly the rows "No, exit" and "Yes, I accept", and the footer as the last line. Any other dialog, or this one reworded, keeps its card. Claude Code 2.1.311 opens the warning on "No, exit", so the key is read from the row the screen marks: Enter when "Yes, I accept" is marked, else Down then Enter. Escape exits Claude here and is never sent.
SEE-ALSO: server/src/agents/bypass_consent.rs, server/src/session_chat_options/detector.rs (the funnel), server/src/session_chat_send/worker.rs (`WaitForComposer`).
*/
/// The keys that accept the warning on this screen, or `None` when the screen is not exactly
/// that warning.
pub(crate) fn bypass_consent_answer_key(screen: &str) -> Option<&'static str> {
    let lines: Vec<String> = screen
        .lines()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    let title = lines.iter().rposition(|line| line == TITLE)?;
    let rest = &lines[title + 1..];
    let [body @ .., decline, accept, footer] = rest else {
        return None;
    };
    if footer != FOOTER
        || body
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            != BODY
    {
        return None;
    }
    let row = |line: &str, label: &str| -> Option<bool> {
        let selected = line.starts_with('❯');
        (line.trim_start_matches('❯').trim() == label).then_some(selected)
    };
    match (row(decline, DECLINE)?, row(accept, ACCEPT)?) {
        (false, true) => Some("\r"),
        (true, false) => Some("\x1b[1;1B\r"),
        _ => None,
    }
}

/// Whether the session's own launch command starts Claude in bypass.
pub(crate) fn session_started_in_bypass(session: &Value) -> bool {
    crate::agents::identity::normalize_agent_id(
        crate::session_chat_follower::session_chat_agent_for_session(session).as_deref(),
    )
    .as_deref()
        == Some("claude")
        && [
            "/launchSettings/agentLaunchPlan/command",
            "/runtimeSettings/agentCommand",
        ]
        .iter()
        .filter_map(|pointer| session.pointer(pointer).and_then(Value::as_str))
        .any(crate::agents::claude_command_starts_in_bypass)
}

static IN_FLIGHT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
static FAILED: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
static LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

fn key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

fn map(
    cell: &'static OnceLock<Mutex<HashMap<String, Instant>>>,
) -> &'static Mutex<HashMap<String, Instant>> {
    cell.get_or_init(|| Mutex::new(HashMap::new()))
}

fn claim(project_id: &str, session_id: &str) -> bool {
    let key = key(project_id, session_id);
    if map(&FAILED)
        .lock()
        .ok()
        .and_then(|failed| failed.get(&key).copied())
        .is_some_and(|at| at.elapsed() < RETRY_AFTER)
    {
        return false;
    }
    map(&IN_FLIGHT)
        .lock()
        .is_ok_and(|mut in_flight| in_flight.insert(key, Instant::now()).is_none())
}

fn release(project_id: &str, session_id: &str, cleared: bool) {
    let key = key(project_id, session_id);
    if let Ok(mut failed) = map(&FAILED).lock() {
        if cleared {
            failed.remove(&key);
        } else {
            failed.insert(key.clone(), Instant::now());
        }
    }
    if let Ok(mut in_flight) = map(&IN_FLIGHT).lock() {
        in_flight.remove(&key);
    }
}

/// Hidden from clients while an answer is in flight, as a remembered folder's trust prompt is.
pub(crate) fn bypass_consent_being_answered(
    project_id: &str,
    session_id: &str,
    notice: &SessionChatTerminalNotice,
) -> bool {
    notice.blocks_input()
        && map(&IN_FLIGHT)
            .lock()
            .is_ok_and(|in_flight| in_flight.contains_key(&key(project_id, session_id)))
}

/// The accept keys for a warning that reads the same on two captures `SETTLE` apart.
async fn settled_key(zmx_name: &str) -> Option<&'static str> {
    let first = capture_session_terminal_text(zmx_name).await?;
    bypass_consent_answer_key(&first)?;
    tokio::time::sleep(SETTLE).await;
    let second = capture_session_terminal_text(zmx_name).await?;
    let key = bypass_consent_answer_key(&second)?;
    (bypass_consent_answer_key(&first) == Some(key)).then_some(key)
}

fn log_answered(project_id: &str, session_id: &str, zmx_name: &str, source: &str) {
    let logger = LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Info,
        event: "claudeBypassWarningAccepted".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "providerSessionId": zmx_name,
            "source": source,
        })),
    });
}

/// Accept the warning on a claimed session, typing the keys once per settled screen. Returns
/// whether the warning is gone.
async fn accept_claimed(project_id: &str, session_id: &str, zmx_name: &str, source: &str) -> bool {
    let warning_gone = || async {
        !capture_session_terminal_text(zmx_name)
            .await
            .is_some_and(|screen| bypass_consent_answer_key(&screen).is_some())
    };
    let mut cleared = false;
    for _ in 0..ATTEMPTS {
        let Some(keys) = settled_key(zmx_name).await else {
            // Gone, or still repainting: look again on the next try.
            cleared = warning_gone().await;
            if cleared {
                break;
            }
            continue;
        };
        if crate::session_chat_send::write_session_chat_payload(
            project_id,
            session_id,
            zmx_name,
            "claude-bypass-consent",
            keys,
        )
        .await
        .is_err()
        {
            continue;
        }
        log_answered(project_id, session_id, zmx_name, source);
        tokio::time::sleep(CLEAR_WAIT).await;
        cleared = warning_gone().await;
        if cleared {
            break;
        }
    }
    release(project_id, session_id, cleared);
    cleared
}

/// For the detection funnel: whether the notice it just classified may be this warning. Only a
/// cheap first look; the answer itself re-reads the whole screen with `bypass_consent_answer_key`.
pub(crate) fn notice_may_be_warning(notice: Option<&SessionChatTerminalNotice>) -> bool {
    notice.is_some_and(|notice| {
        notice.blocks_input()
            && notice
                .screen_tail
                .as_deref()
                .is_some_and(|tail| tail.contains(TITLE))
    })
}

/// The same, on a session that started in bypass.
pub(crate) fn funnel_may_answer(
    session: &Value,
    notice: Option<&SessionChatTerminalNotice>,
) -> bool {
    notice_may_be_warning(notice) && session_started_in_bypass(session)
}

/// From the detection funnel: claim the session now, so the card is hidden from the first
/// publish, then answer in the background and redetect so clients see the composer.
pub(crate) fn dispatch_bypass_consent(
    detector: crate::session_chat_options::SessionChatOptionDetector,
    project_id: &str,
    session_id: &str,
    zmx_name: String,
    agent: Option<&str>,
) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    if !claim(project_id, session_id) {
        return;
    }
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let agent = agent.map(str::to_string);
    handle.spawn(async move {
        accept_claimed(&project_id, &session_id, &zmx_name, "detection").await;
        detector
            .detect(&project_id, &session_id, agent.as_deref(), true)
            .await;
        detector.publish_screen_state(&project_id, &session_id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The warning exactly as Claude Code 2.1.311 paints it at 160 columns (captured from a
    /// throwaway config folder), with the cursor on its default row.
    const WARNING_2_1_311: &str = "────────────────────────────────────────────────────────────────────────────────

  WARNING: Claude Code running in Bypass Permissions mode

  In Bypass Permissions mode, Claude Code will not ask for your approval before running potentially dangerous commands.
  This mode should only be used in a sandboxed container/VM that has restricted internet access and can easily be restored if damaged.

  By proceeding, you accept all responsibility for actions taken while running in Bypass Permissions mode.

  https://code.claude.com/docs/en/security

  ❯ No, exit
    Yes, I accept

  Enter to confirm · Esc to cancel


";

    /// The screen from the report, with "Yes, I accept" marked.
    const WARNING_ACCEPT_MARKED: &str = "  WARNING: Claude Code running in Bypass Permissions mode

  In Bypass Permissions mode, Claude Code will not ask for your approval before running potentially dangerous commands.
  This mode should only be used in a sandboxed container/VM that has restricted internet access and can easily be restored if damaged.

  By proceeding, you accept all responsibility for actions taken while running in Bypass Permissions mode.

  https://code.claude.com/docs/en/security

    No, exit
  ❯ Yes, I accept

  Enter to confirm · Esc to cancel
";

    #[test]
    fn the_exact_warning_is_accepted_from_either_row() {
        assert_eq!(
            bypass_consent_answer_key(WARNING_2_1_311),
            Some("\x1b[1;1B\r")
        );
        assert_eq!(bypass_consent_answer_key(WARNING_ACCEPT_MARKED), Some("\r"));
    }

    /// Claude Code 2.1.311 at 100 columns, where its text wraps.
    const WARNING_2_1_311_100_COLUMNS: &str = "────────────────────────────────────────────────────────────────────────────────────────────────────

  WARNING: Claude Code running in Bypass Permissions mode

  In Bypass Permissions mode, Claude Code will not ask for your approval before running
  potentially dangerous commands.
  This mode should only be used in a sandboxed container/VM that has restricted internet access
  and can easily be restored if damaged.

  By proceeding, you accept all responsibility for actions taken while running in Bypass
  Permissions mode.

  https://code.claude.com/docs/en/security

  ❯ No, exit
    Yes, I accept

  Enter to confirm · Esc to cancel
";

    #[test]
    fn a_narrow_terminal_wrapping_the_text_still_matches() {
        assert_eq!(
            bypass_consent_answer_key(WARNING_2_1_311_100_COLUMNS),
            Some("\x1b[1;1B\r")
        );
        let wrapped = WARNING_ACCEPT_MARKED.replace(
            "will not ask for your approval ",
            "will not ask for your approval\n  ",
        );
        assert_eq!(bypass_consent_answer_key(&wrapped), Some("\r"));
    }

    #[test]
    fn near_misses_are_not_answered() {
        let cases = [
            // Reworded text.
            WARNING_ACCEPT_MARKED.replace("accept all responsibility", "accept responsibility"),
            // Another title.
            WARNING_ACCEPT_MARKED.replace("Bypass Permissions mode\n", "Auto mode\n"),
            // A third row.
            WARNING_ACCEPT_MARKED.replace(
                "  ❯ Yes, I accept\n",
                "  ❯ Yes, I accept\n    Yes, always\n",
            ),
            // Rows swapped.
            WARNING_ACCEPT_MARKED.replace(
                "    No, exit\n  ❯ Yes, I accept",
                "  ❯ Yes, I accept\n    No, exit",
            ),
            // No row marked.
            WARNING_ACCEPT_MARKED.replace("  ❯ Yes, I accept", "    Yes, I accept"),
            // Something painted below the footer (a composer: the dialog is transcript text).
            format!("{WARNING_ACCEPT_MARKED}────\n❯ \n────\n  ⏵⏵ bypass permissions on\n"),
            // Footer missing.
            WARNING_ACCEPT_MARKED.replace("  Enter to confirm · Esc to cancel\n", ""),
            // The title quoted in a reply, with nothing else.
            format!("⏺ Claude printed {TITLE} earlier.\n\n❯ \n"),
        ];
        for screen in &cases {
            assert_eq!(bypass_consent_answer_key(screen), None, "{screen}");
        }
    }

    #[test]
    fn real_permission_prompts_and_trust_prompts_are_not_answered() {
        let bash_permission = "⏺ Bash(rm -rf build)\n\n────\n Bash command\n\n   rm -rf build\n   Remove the build folder\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, and don't ask again for rm commands in this project\n   3. No, and tell Claude what to do differently (esc)\n\n";
        let trust = " Accessing workspace:\n\n /tmp/proj\n\n Quick safety check: Is this a project you created or one you trust?\n\n Claude Code'll be able to read, edit, and execute files here.\n\n Security guide\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";
        let api_key = "  Detected a custom API key in your environment\n  ANTHROPIC_API_KEY: sk-ant-...\n  Do you want to use this API key?\n    Yes\n  ❯ No (recommended)\n  Enter to confirm · Esc to cancel\n";
        let managed = "  Managed settings require approval\n\n  ❯ Yes, I accept\n    No, exit\n\n  Enter to confirm · Esc to cancel\n";
        for screen in [bash_permission, trust, api_key, managed] {
            assert_eq!(bypass_consent_answer_key(screen), None, "{screen}");
        }
    }

    #[test]
    fn the_funnel_sees_the_warning_as_a_blocking_notice_with_its_title() {
        let notice = crate::session_chat_notice::classify_session_chat_terminal_notice(
            Some("claude"),
            WARNING_2_1_311,
        )
        .expect("the warning is a notice");
        let session = json!({
            "agentId": "claude",
            "launchSettings": { "agentLaunchPlan": { "command": "claude --dangerously-skip-permissions" } },
        });
        assert!(funnel_may_answer(&session, Some(&notice)), "{notice:?}");
        let default_mode = json!({
            "agentId": "claude",
            "launchSettings": { "agentLaunchPlan": { "command": "claude" } },
        });
        assert!(!funnel_may_answer(&default_mode, Some(&notice)));
    }

    #[test]
    fn only_sessions_started_in_bypass_are_answered() {
        let session = |agent: &str, command: &str| {
            json!({
                "agentId": agent,
                "launchSettings": { "agentLaunchPlan": { "command": command } },
            })
        };
        assert!(session_started_in_bypass(&session(
            "claude",
            "claude --dangerously-skip-permissions"
        )));
        assert!(session_started_in_bypass(&session(
            "claude",
            "claude --permission-mode bypassPermissions"
        )));
        assert!(!session_started_in_bypass(&session("claude", "claude")));
        assert!(!session_started_in_bypass(&session(
            "claude",
            "claude --allow-dangerously-skip-permissions"
        )));
        assert!(!session_started_in_bypass(&session(
            "codex",
            "codex --dangerously-skip-permissions"
        )));
    }
}
