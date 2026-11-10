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
    // An unreadable snapshot proves nothing.
    (agent_process_running(&session, false) == Some(false))
        .then(|| with_missing_agent_command(notice))
}

/// Whether the session's agent came up, wrote its conversation and is gone now: the screen is back
/// at a shell prompt and no agent process runs in the session, the same reading as a failed start
/// ([`agent_start_failure`]) for a session that has a conversation, held for the same settle time.
/// It is no card: the chat only stops waiting on an agent that is not there (`agentGone` on the
/// chat's state). Once confirmed it stays until an agent process is back in the session or the
/// conversation grows, whatever the screen shows: a command typed at that shell (`sleep 100`) is
/// not the agent coming back. The process read is the shared cached snapshot, since a dead agent
/// whose Working never cleared is probed about every second.
///
/// CDXC:SessionChat 2026-11-09 DECISION:
/// Sebastian picked a silent signal (Q43, option B) for an agent that dies without the "agent exited" notice (killed, crashed): gxserver tells the chat the agent is not running and Agent work Quiet shows the words it held back, as it does on that notice. No card, no notice and nothing new on screen, and it must not reach the account recovery that keys off the "agent exited" notice, so it is not one.
/// SEE-ALSO: packages/gx-chat-core/src/session/working.rs (`agent_exited` reads `agentGone`), packages/gx-protocol/src/chat.rs (`ChatSideState::agent_gone`).
pub(crate) fn agent_left_conversation(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    screen_text: &str,
) -> bool {
    let key = format!("{project_id}:{session_id}");
    let shell_prompt = agent_start_failure_notice(screen_text).is_some();
    {
        let mut watches = agent_gone_watches();
        forget_untouched(&mut watches, Instant::now());
        // Nothing seen yet and the agent's screen is up: no read at all.
        if !watches.contains_key(&key) && !shell_prompt {
            return false;
        }
    }
    // The reads run without the lock; the step below applies them to the record as it is then.
    let session = repository
        .get_session(project_id, session_id)
        .ok()
        .flatten();
    let conversation = session
        .as_ref()
        .filter(|session| !crate::agentbox::is_agentbox_session(session))
        .map(conversation_file)
        .filter(|(transcript, _)| *transcript == Transcript::Written);
    let (Some(session), Some((_, path))) = (session, conversation) else {
        agent_gone_watches().remove(&key);
        return false;
    };
    let reading = GoneReading {
        shell_prompt,
        agent_process: agent_process_running(&session, true),
        conversation_len: path
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|metadata| metadata.len()),
    };
    record_reading(&mut agent_gone_watches(), key, reading, Instant::now())
}

/// Whether this session's agent is confirmed gone: what a probe that could not read the screen
/// (the capture failed or was cut) says, so a confirmed absence outlives it.
pub(crate) fn agent_gone_confirmed(project_id: &str, session_id: &str) -> bool {
    agent_gone_watches()
        .get(&format!("{project_id}:{session_id}"))
        .is_some_and(|stored| stored.watch.confirmed_at.is_some())
}

/// Drops the session's record: its agent was started again or the session went away.
pub(crate) fn forget_agent_gone(project_id: &str, session_id: &str) {
    agent_gone_watches().remove(&format!("{project_id}:{session_id}"));
}

/// Test seam: records the session's agent as confirmed gone, as a probe past the settle would.
#[cfg(test)]
pub(crate) fn confirm_agent_gone_for_test(project_id: &str, session_id: &str) {
    let now = Instant::now();
    agent_gone_watches().insert(
        format!("{project_id}:{session_id}"),
        StoredWatch {
            watch: GoneWatch {
                since: now,
                confirmed_at: Some(None),
            },
            touched: now,
        },
    );
}

/// Per session, `project:session`: an agent seen gone, waiting out the settle or confirmed.
static AGENT_GONE: LazyLock<Mutex<HashMap<String, StoredWatch>>> = LazyLock::new(Default::default);

fn agent_gone_watches() -> std::sync::MutexGuard<'static, HashMap<String, StoredWatch>> {
    AGENT_GONE.lock().unwrap_or_else(|error| error.into_inner())
}

/// How long a record outlives the last probe of its session. A followed chat whose agent died is
/// probed every few seconds; a session nobody probes for this long has ended or is not watched,
/// and a probe after that reads it afresh.
const AGENT_GONE_UNTOUCHED: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, Debug)]
struct StoredWatch {
    watch: GoneWatch,
    touched: Instant,
}

fn forget_untouched(watches: &mut HashMap<String, StoredWatch>, now: Instant) {
    watches.retain(|_, stored| now.duration_since(stored.touched) < AGENT_GONE_UNTOUCHED);
}

/// Applies one probe's reading to the session's record as it stands now, under the lock, so probes
/// that overlap never move a confirmed absence back: only evidence in the reading itself (an agent
/// process, a grown conversation) releases it, and the answer is the record's own.
fn record_reading(
    watches: &mut HashMap<String, StoredWatch>,
    key: String,
    reading: GoneReading,
    now: Instant,
) -> bool {
    let current = watches.get(&key).map(|stored| stored.watch);
    let (next, gone) = advance_gone_watch(current, reading, now);
    match next {
        Some(watch) => {
            watches.insert(
                key,
                StoredWatch {
                    watch,
                    touched: now,
                },
            );
        }
        None => {
            watches.remove(&key);
        }
    }
    gone
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GoneWatch {
    /// When the screen was first seen back at the shell with no agent process.
    since: Instant,
    /// Once confirmed, the conversation's size then (`None` when unreadable).
    confirmed_at: Option<Option<u64>>,
}

/// What one probe saw of a session whose agent wrote its conversation.
#[derive(Clone, Copy, Debug)]
struct GoneReading {
    shell_prompt: bool,
    /// Whether an agent process runs in the session; `None` when the snapshot is unreadable.
    agent_process: Option<bool>,
    conversation_len: Option<u64>,
}

/// One step of [`agent_left_conversation`]: the watch to keep and whether the agent is gone.
fn advance_gone_watch(
    watch: Option<GoneWatch>,
    reading: GoneReading,
    now: Instant,
) -> (Option<GoneWatch>, bool) {
    // An agent process in the session again: it runs.
    if reading.agent_process == Some(true) {
        return (None, false);
    }
    if let Some(
        watch @ GoneWatch {
            confirmed_at: Some(len),
            ..
        },
    ) = watch
    {
        // A conversation that grew was written by an agent that came back.
        let grew = matches!((reading.conversation_len, len), (Some(now_len), Some(then)) if now_len > then);
        return if grew {
            (None, false)
        } else {
            (Some(watch), true)
        };
    }
    if !reading.shell_prompt {
        return (None, false);
    }
    // An unreadable snapshot proves nothing either way.
    if reading.agent_process.is_none() {
        return (watch, false);
    }
    let watch = watch.unwrap_or(GoneWatch {
        since: now,
        confirmed_at: None,
    });
    if now.duration_since(watch.since) < START_FAILURE_SETTLE {
        return (Some(watch), false);
    }
    (
        Some(GoneWatch {
            confirmed_at: Some(reading.conversation_len),
            ..watch
        }),
        true,
    )
}

/// Whether an agent process runs in the session's terminal, read from the snapshot the send heal
/// trusts (`cached`: the shared two-second snapshot); `None` when it is unreadable.
fn agent_process_running(session: &Value, cached: bool) -> Option<bool> {
    let zmx_name = crate::zmx::provider_zmx_session_name(session).ok()?;
    let names = std::slice::from_ref(&zmx_name);
    let home = crate::resume_lookup::home_dir();
    let identities = if cached {
        crate::zmx::read_cached_zmx_session_process_identities(names, &home)
    } else {
        crate::zmx::read_zmx_session_process_identities(names, &home)
    };
    identities
        .ok()
        .map(|identities| identities.contains_key(&zmx_name))
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
    conversation_file(session).0
}

/// [`conversation_on_disk`], with the conversation's file when Ghostex found one.
fn conversation_file(session: &Value) -> (Transcript, Option<std::path::PathBuf>) {
    let Some(agent_session_id) = read_runtime_text(session, "agentSessionId") else {
        return (Transcript::Missing, None);
    };
    let agent = crate::session_chat_follower::session_chat_agent_for_session(session);
    // An agent whose transcript Ghostex cannot find reported an id, which only a running agent does.
    let Some(transcript_agent) =
        crate::session_chat::resolve_session_chat_transcript_agent(agent.as_deref())
    else {
        return (Transcript::Written, None);
    };
    match crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        Some(&agent_session_id),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )
    .filter(|path| path.is_file())
    {
        None => (Transcript::Missing, None),
        Some(path) if crate::agent_transcripts::transcript_has_records(&path) => {
            (Transcript::Written, Some(path))
        }
        Some(path) => (Transcript::Unreadable, Some(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reading(shell_prompt: bool, agent_process: Option<bool>, len: u64) -> GoneReading {
        GoneReading {
            shell_prompt,
            agent_process,
            conversation_len: Some(len),
        }
    }

    /// Killed mid-call: the shell's prompt with no agent process, held for the settle time, then
    /// the agent is gone until it comes back, whatever is typed at the shell.
    #[test]
    fn a_confirmed_absence_holds_until_an_agent_comes_back() {
        let start = Instant::now();
        let step = |watch, reading, after: u64| {
            advance_gone_watch(watch, reading, start + Duration::from_secs(after))
        };
        // A resumed conversation shows the prompt a moment before its agent launches.
        let (watch, gone) = step(None, reading(true, Some(false), 100), 0);
        assert!(!gone, "not before the settle time");
        let (watch, gone) = step(watch, reading(true, Some(false), 100), 3);
        assert!(!gone);
        let (watch, gone) = step(watch, reading(true, Some(false), 100), 9);
        assert!(gone, "confirmed after the settle time");
        // `sleep 100` typed at the shell: no prompt, still no agent.
        let (watch, gone) = step(watch, reading(false, Some(false), 100), 10);
        assert!(gone, "a command at the shell is not the agent coming back");
        let (watch, gone) = step(watch, reading(false, None, 100), 11);
        assert!(gone, "an unreadable snapshot changes nothing");
        // The agent is started again.
        let (watch, gone) = step(watch, reading(false, Some(true), 100), 12);
        assert!(!gone && watch.is_none());
        // Confirmed again, then its conversation grows: an agent wrote it.
        let (watch, _) = step(None, reading(true, Some(false), 100), 20);
        let (watch, gone) = step(watch, reading(true, Some(false), 100), 29);
        assert!(gone);
        let (watch, gone) = step(watch, reading(false, Some(false), 140), 30);
        assert!(!gone && watch.is_none());
    }

    /// Two probes overlap: the slower one read the screen while the absence was still settling (a
    /// command typed at the shell since), and lands after the faster one confirmed it. The
    /// confirmed absence stands and the slower probe answers with it.
    #[test]
    fn an_overlapping_probe_never_moves_a_confirmed_absence_back() {
        let start = Instant::now();
        let mut watches = HashMap::new();
        let key = || "P:S".to_string();
        assert!(!record_reading(
            &mut watches,
            key(),
            reading(true, Some(false), 5),
            start
        ));
        let slow = reading(false, Some(false), 5);
        let fast = reading(true, Some(false), 5);
        assert!(record_reading(
            &mut watches,
            key(),
            fast,
            start + Duration::from_secs(9)
        ));
        assert!(
            record_reading(&mut watches, key(), slow, start + Duration::from_secs(9)),
            "the slower probe answers with the confirmed absence"
        );
        assert!(watches[&key()].watch.confirmed_at.is_some());
    }

    /// A record nobody probed for an hour goes; one probed keeps going.
    #[test]
    fn records_of_sessions_nobody_probes_are_forgotten() {
        let start = Instant::now();
        let mut watches = HashMap::new();
        record_reading(
            &mut watches,
            "P:A".into(),
            reading(true, Some(false), 1),
            start,
        );
        let later = start + AGENT_GONE_UNTOUCHED;
        record_reading(
            &mut watches,
            "P:B".into(),
            reading(true, Some(false), 1),
            later,
        );
        forget_untouched(&mut watches, later);
        assert!(!watches.contains_key("P:A") && watches.contains_key("P:B"));
    }

    /// The prompt goes before the settle time: nothing is held.
    #[test]
    fn a_prompt_that_goes_before_the_settle_time_starts_over() {
        let start = Instant::now();
        let (watch, _) = advance_gone_watch(None, reading(true, Some(false), 1), start);
        let (watch, gone) = advance_gone_watch(
            watch,
            reading(false, Some(false), 1),
            start + Duration::from_secs(2),
        );
        assert!(!gone && watch.is_none());
    }

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
