//! An agent whose start command ended before the agent came up: the chat shows why instead of
//! waiting at "starting".

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::server::read_runtime_text;
use crate::session_chat_notice::{agent_start_failure_notice, SessionChatTerminalNotice};

/// How long a session must keep showing a failed start before the card appears. A zsh with
/// Powerlevel10k draws its cached instant prompt while `.zshrc` loads, before the agent's process
/// exists (zmx/zsh_startup.rs), and that screen must not read as a failed start.
const START_FAILURE_SETTLE: Duration = Duration::from_secs(8);

/// When each session was first seen in the failed-start state, keyed `project:session`.
static FIRST_SEEN: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);

/// CDXC:SessionChat 2026-10-16 WHY:
/// A customer's Claude account session ran `cswap run 3 --share-history -- …` on Windows, cswap printed "--share-history is not supported on Windows yet" and exited, and PowerShell stayed open at its prompt. Every screen rule matches a known agent's own words, so nothing classified cswap's, the chat sat at "starting" with "hello" held as a startup send for good, and the send heal could not restart a session that had no conversation. Any start command can end that way (a missing CLI, a wrapper's error, a bad flag) on every platform, so the card does not read the error: the screen is back at a shell prompt, no agent process runs in the session (the snapshot the send heal trusts), and the session has no written conversation, so nothing ever came up. The card shows the terminal's last lines, holds the queue, and offers Try again; restarting on its own would fail the same way. A session that has a conversation keeps the send heal's restart (session_chat_queue_runtime/send_heal.rs).
/// SEE-ALSO: session_chat_notice/start_failure.rs (`agent_start_failure_notice`, `start_failed`), session_chat_queue_runtime/scheduler.rs (holds rows behind the card), session_chat_agent_restart.rs (Try again).
pub(crate) fn detect_agent_start_failure(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    screen_text: &str,
) -> Option<SessionChatTerminalNotice> {
    let key = format!("{project_id}:{session_id}");
    let failure = agent_start_failure(repository, project_id, session_id, screen_text);
    let mut first_seen = FIRST_SEEN.lock().unwrap_or_else(|error| error.into_inner());
    match failure {
        Some(notice) => {
            let since = *first_seen.entry(key).or_insert_with(Instant::now);
            (since.elapsed() >= START_FAILURE_SETTLE).then_some(notice)
        }
        None => {
            first_seen.remove(&key);
            None
        }
    }
}

/// `detect_agent_start_failure` without the settle time, for a user's Try again on the card.
pub(crate) fn agent_start_failure(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    screen_text: &str,
) -> Option<SessionChatTerminalNotice> {
    let mut notice = agent_start_failure_notice(screen_text)?;
    let session = repository.get_session(project_id, session_id).ok()??;
    if let Some(agent_name) = card_agent_name(&session) {
        notice.title = format!("{agent_name} couldn't start");
    }
    // A box runs its agent where this computer's process table cannot see it.
    let transcript = conversation_on_disk(&session);
    if crate::agentbox::is_agentbox_session(&session) || transcript == Transcript::Written {
        return None;
    }
    if transcript == Transcript::Unreadable {
        notice.detail = Some(LOST_CHAT_DETAIL.to_string());
    }
    let zmx_name = crate::zmx::provider_zmx_session_name(&session).ok()?;
    let running = crate::zmx::read_zmx_session_process_identities(
        std::slice::from_ref(&zmx_name),
        &crate::resume_lookup::home_dir(),
    )
    // An unreadable snapshot proves nothing.
    .map_or(true, |identities| identities.contains_key(&zmx_name));
    (!running).then(|| with_missing_agent_command(notice))
}

/// The name the card gives the session's agent (Q1122): "Claude Code couldn't start", not "The
/// agent couldn't start". It is the CLI the user picked, not its transcript family: an OMP session
/// reads Pi's transcripts, and its card said "Pi couldn't start" (GitHub #456 follow-up).
fn card_agent_name(session: &Value) -> Option<&'static str> {
    match crate::session_chat_composer::session_chat_composer_agent_id(session)?.as_str() {
        "claude" => Some("Claude Code"),
        agent => crate::agents::default_agent_session_title_name(agent),
    }
}

/// The failed-start card, reworded when what failed is an agent's own command not being found
/// (`agent_command_missing_notice`).
fn with_missing_agent_command(notice: SessionChatTerminalNotice) -> SessionChatTerminalNotice {
    let Some(command) = notice
        .error_lines
        .as_deref()
        .and_then(crate::session_chat_notice::missing_start_command)
    else {
        return notice;
    };
    let Some(definition) = crate::agent_cli::catalog::CATALOG
        .iter()
        .find(|definition| definition.binary == command)
    else {
        return notice;
    };
    let agent_name = crate::agents::default_agent_session_title_name(&definition.agent_id)
        .unwrap_or(definition.binary.as_str());
    let home = crate::resume_lookup::home_dir();
    // A session planned before Ghostex ran mise-installed agents through mise: Try again now does.
    if crate::agent_cli::launch_path::with_reachable_program(&command) != command {
        let mut notice = notice;
        notice.title = format!("{agent_name} is ready to start again");
        notice.detail = Some(format!(
            "{agent_name} was installed with mise, which this terminal hadn't set up. Try again starts it through mise."
        ));
        return notice;
    }
    let installed_at = crate::agent_cli::process::resolve_agent(
        &definition.binary,
        &definition.install_dirs(&home),
        &home,
    );
    crate::session_chat_notice::agent_command_missing_notice(
        notice,
        agent_name,
        &command,
        installed_at.as_deref(),
    )
}

/// What a start failure's card says when the chat's file is on disk with nothing in it (`transcript_has_records`): Try again resumes the same empty file.
pub(crate) const LOST_CHAT_DETAIL: &str =
    "Its saved chat is empty, so it can't reopen. This happens when the computer shuts down before saving; start a new chat.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transcript {
    Missing,
    /// On disk but without one record (`agent_transcripts::transcript_has_records`).
    Unreadable,
    Written,
}

/// Whether the session's agent has written a conversation, so it came up at least once.
fn conversation_on_disk(session: &Value) -> Transcript {
    let Some(agent_session_id) = read_runtime_text(session, "agentSessionId") else {
        return Transcript::Missing;
    };
    let agent = crate::session_chat_follower::session_chat_agent_for_session(session);
    // An agent whose transcript Ghostex cannot find reported an id, which only a running agent does.
    let Some(transcript_agent) =
        crate::session_chat::resolve_session_chat_transcript_agent(agent.as_deref())
    else {
        return Transcript::Written;
    };
    match crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        Some(&agent_session_id),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )
    .filter(|path| path.is_file())
    {
        None => Transcript::Missing,
        Some(path) if crate::agent_transcripts::transcript_has_records(&path) => {
            Transcript::Written
        }
        Some(_) => Transcript::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_card_names_the_cli_the_user_picked() {
        for (session, name) in [
            (json!({"agentId": "omp"}), Some("OMP")),
            (json!({"agentId": "pi"}), Some("Pi")),
            (json!({"agentId": "empryo"}), Some("Empryo")),
            (json!({"agentId": "claude"}), Some("Claude Code")),
            (json!({"agentId": "codex"}), Some("Codex")),
            // A sidebar agent configuration runs the CLI its icon names.
            (
                json!({"agentId": "custom-1", "launchSettings": {"icon": "omp"}}),
                Some("OMP"),
            ),
            (json!({}), None),
        ] {
            assert_eq!(card_agent_name(&session), name, "{session}");
        }
    }
}
