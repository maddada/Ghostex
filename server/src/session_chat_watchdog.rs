/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Chat sends are fire-and-forget: `write_session_chat_payload` succeeds the moment
zmx accepts the bytes, so a message typed into a dead login screen, a trust
dialog or a shell where the agent already exited disappears without a trace.

This module is the other half of `session_chat_notice.rs`: instead of reading
what the screen SAYS, it checks what the agent RECORDED. Every text send samples
the session transcript's byte length first, and after the send completes a
watchdog task watches the bytes appended past that offset for the message it
just sent. Delivery proven ⇒ silence. Nothing after 10s ⇒ one terminal capture,
classified, and published as a `terminalNotice` so the user sees the login
screen or the crashed CLI that swallowed the message.

Hard boundaries, because this runs behind every single chat send:
  - it NEVER retries a send, never writes to the terminal, and never delays or
    blocks the send path (it is spawned after the send has already resolved);
  - it spawns at most ONE `zmx history` capture, and only at the timeout;
  - a new send supersedes the previous watchdog for that session, so a fast
    typist has one watchdog, not five.

Delivery is checked in two tiers because neither alone is complete:
  a. decoded User-role messages from the shared incremental reader — the normal
     case for both agents;
  b. a JSON-escaped substring scan of the appended raw bytes — catches Claude's
     `queue-operation` enqueue rows (typed while a turn runs) and Codex's
     `response_item` message lane, neither of which the decoders surface.

CDXC:SessionChatTerminalNotices 2026-08-24: a third tier answers the opposite
question — not "can delivery be proven?" but "was something ELSE submitted in
its place?" (`observe_mismatched_input`). It is the only tier with affirmative
evidence, so it does not wait out the deadline and it is not silenced by the
"the agent was already working" suppression, which is precisely what used to
swallow an empty submit.
*/

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use serde_json::{Map, Value};

use crate::session_chat::{
    is_noise_message, parse_json_object, read_incremental_transcript_messages,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    session_chat_line_decoder, text_block, SessionChatBlock, SessionChatIncrementalState,
    SessionChatLineDecoder, SessionChatMessage, SessionChatRole, SessionChatSource,
    SessionChatTranscriptAgent,
};
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, clear_session_chat_watchdog_notice,
    session_chat_delivery_mismatch_notice, session_chat_notice_key,
    session_chat_screen_shows_queued_input, session_chat_terminal_screen_tail,
    session_chat_watchdog_notice, set_session_chat_watchdog_notice, SessionChatTerminalNotice,
    SessionChatTerminalNoticeAction, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource, SESSION_CHAT_NOTICE_AGENT_EXITED,
    SESSION_CHAT_NOTICE_DELIVERY_FAILED, SESSION_CHAT_NOTICE_QUEUED_INPUT,
};
use crate::session_chat_options::session_chat_option_agent;

/// Poll cadence. The transcript is flushed per line, so a delivered message is
/// normally visible on the first tick.
const WATCHDOG_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Alarm budget. Claude records the user turn in well under a second; Codex
/// writes it before it even issues the model request. 10s is slack, not hope.
const WATCHDOG_DEADLINE: Duration = Duration::from_secs(10);
/// The raw scan keeps the OLDEST appended bytes: the record we are looking for
/// is written first, and a chatty turn can append megabytes behind it.
const WATCHDOG_RAW_SCAN_LIMIT_BYTES: u64 = 512 * 1024;
/// Below this length a raw substring scan stops being evidence — "ok" appears
/// in everything. Short sends still match through the decoded tier.
const WATCHDOG_RAW_SCAN_MIN_CHARS: usize = 12;
/// A decoded turn that carries most of what was sent counts as delivered: TUIs
/// trim and re-wrap, and a partial match still proves the message landed.
const WATCHDOG_PREFIX_MATCH_PERCENT: usize = 80;
/*
CDXC:SessionChatTerminalNotices 2026-08-24:
Once the transcript has recorded a user turn that is NOT ours AND the agent has
started answering it, waiting out the rest of the deadline buys nothing: the
composer has already been submitted past our message. These extra polls exist
only so a transcript that writes the agent's rows before the user row it is
answering cannot be read as a mismatch.
*/
const WATCHDOG_MISMATCH_GRACE_POLLS: u32 = 2;
/// Registry files are one per live CLI; a machine has a handful, not thousands.
const CLAUDE_REGISTRY_SCAN_LIMIT: usize = 256;

// ---------------------------------------------------------------------------
// Collaborators owned by server.rs
// ---------------------------------------------------------------------------

/// Session facts the escalation needs, read fresh at the deadline.
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionChatWatchdogLiveState {
    pub running: bool,
    pub working: bool,
}

/// Pushes whatever notice the session should be showing right now to live
/// followers. The watchdog mutates the store and then calls this; it never
/// builds frames itself.
pub type SessionChatWatchdogPublisher = Arc<dyn Fn() + Send + Sync>;

/// Blocking read of the session row (SQLite), supplied by server.rs.
pub type SessionChatWatchdogStateReader =
    Arc<dyn Fn() -> SessionChatWatchdogLiveState + Send + Sync>;

// ---------------------------------------------------------------------------
// Send probe
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Sampled BEFORE the send is enqueued: everything appended to the transcript from
here on is a candidate for "the message arrived". Sampling afterwards would race
the agent's own write.
*/
pub struct SessionChatSendProbe {
    project_id: String,
    session_id: String,
    zmx_name: String,
    agent: Option<String>,
    transcript_agent: SessionChatTranscriptAgent,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    /// `None` when the agent has not created its transcript yet (Codex creates
    /// the rollout lazily); the watchdog re-resolves it every tick.
    transcript_path: Option<PathBuf>,
    transcript_offset: u64,
    /*
    CDXC:SessionChatTerminalNotices 2026-08-20:
    What the CLI itself swallows instead of sending to the model, if anything.
    This watchdog reasons about transcript writes, and an intercepted message
    either lands there in a different shape or never lands at all:
      - Claude logs `/rename` as a `system` / `local_command` row and `!ls` as a
        `<bash-input>` row, in both cases WITHOUT a user turn;
      - Codex records nothing whatsoever — verified against both its source
        (`SlashCommand` is dispatched entirely inside the TUI, `!cmd` runs
        through `submit_shell_command_with_history` which only touches the
        message history file) and every rollout on this machine.
    Both facts are handled where they matter: extra delivery proof below, and a
    suppression for the guess-based verdicts in `escalate_undelivered_send`.
    */
    intercepted: Option<InterceptedInput>,
    /// Wall-clock send time, for the deadline's last-chance scan: only a
    /// transcript file written after this instant can testify about the send.
    sent_at: std::time::SystemTime,
    text: String,
}

impl SessionChatSendProbe {
    /// `None` disables the watchdog for this send: only the catalogued agents
    /// (claude/openclaude/codex) have verified transcript-write semantics, and
    /// an image-only send has no text to look for.
    pub async fn sample(
        project_id: &str,
        session_id: &str,
        zmx_name: &str,
        agent: Option<&str>,
        agent_session_id: Option<&str>,
        agent_session_path: Option<&str>,
        text: &str,
    ) -> Option<Self> {
        if session_chat_option_agent(agent).is_none() {
            return None;
        }
        if text.trim().is_empty() {
            return None;
        }
        let transcript_agent = resolve_session_chat_transcript_agent(agent)?;
        /*
        Resolution walks the agent's transcript tree when hooks never reported a
        path (Codex often does not), so it runs on a blocking thread: the send
        handler is an interactive path and must not stall an executor thread on
        a directory sweep.
        */
        let (transcript_path, transcript_offset) = {
            let agent_session_id = agent_session_id.map(str::to_string);
            let agent_session_path = agent_session_path.map(str::to_string);
            tokio::task::spawn_blocking(move || {
                let path = resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                );
                let offset = path
                    .as_deref()
                    .and_then(|path| std::fs::metadata(path).ok())
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                (path, offset)
            })
            .await
            .ok()?
        };
        Some(Self {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            zmx_name: zmx_name.to_string(),
            agent: agent.map(str::to_string),
            transcript_agent,
            agent_session_id: agent_session_id.map(str::to_string),
            agent_session_path: agent_session_path.map(str::to_string),
            transcript_path,
            transcript_offset,
            intercepted: InterceptedInput::detect(text),
            sent_at: std::time::SystemTime::now(),
            text: text.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Per-session registry (one watchdog per session, newest wins)
// ---------------------------------------------------------------------------

struct SessionChatWatchdogEntry {
    generation: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

static SESSION_CHAT_WATCHDOGS: OnceLock<Mutex<HashMap<String, SessionChatWatchdogEntry>>> =
    OnceLock::new();

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Starts the watchdog for a completed send. Supersedes the session's previous
watchdog (abort + generation bump, the same two-layer cancellation the send
queue uses), and the new task's first act is to retire any notice the previous
one published: the user just proved the terminal accepts input.
Must be called from inside the tokio runtime.
*/
pub fn start_session_chat_send_watchdog(
    probe: SessionChatSendProbe,
    publish: SessionChatWatchdogPublisher,
    read_state: SessionChatWatchdogStateReader,
) {
    let probe = Arc::new(probe);
    let (project_id, session_id) = (probe.project_id.clone(), probe.session_id.clone());
    register_session_chat_watchdog_task(
        &project_id,
        &session_id,
        move |generation, my_generation| {
            tokio::spawn(run_session_chat_send_watchdog(
                probe,
                publish,
                read_state,
                generation,
                my_generation,
            ))
        },
    );
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
The flagship case never reaches the watchdog above: when the agent CLI is dead
or was never in the pane, the send fails INSIDE the Ctrl+G preservation
handshake (or on the zmx write), so the handler returns an error before any
delivery verification exists and the user gets a generic toast with no
explanation of what the terminal is showing.

This is that missing escalation: the same one-capture verdict the watchdog takes
at its deadline, taken once, immediately, for a send the terminal refused. It
does not retry the send, does not touch the terminal, and takes exactly one
capture. It registers as this session's watchdog so a later send supersedes it
the same way it supersedes a real one.
*/
pub fn escalate_failed_session_chat_send(
    probe: SessionChatSendProbe,
    publish: SessionChatWatchdogPublisher,
    read_state: SessionChatWatchdogStateReader,
) {
    let probe = Arc::new(probe);
    let (project_id, session_id) = (probe.project_id.clone(), probe.session_id.clone());
    register_session_chat_watchdog_task(
        &project_id,
        &session_id,
        move |generation, my_generation| {
            tokio::spawn(async move {
                if generation.load(Ordering::SeqCst) != my_generation {
                    return;
                }
                escalate_undelivered_send(
                    &probe,
                    probe.transcript_path.is_some(),
                    &publish,
                    &read_state,
                    UndeliveredSendReason::WriteFailed,
                )
                .await;
            })
        },
    );
}

/// One watchdog per session, newest wins: bump the generation, abort whatever
/// was running, and record the replacement.
fn register_session_chat_watchdog_task(
    project_id: &str,
    session_id: &str,
    spawn: impl FnOnce(Arc<AtomicU64>, u64) -> tokio::task::JoinHandle<()>,
) {
    let watchdogs = SESSION_CHAT_WATCHDOGS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut map) = watchdogs.lock() else {
        return;
    };
    map.retain(|_, entry| !entry.task.is_finished());
    let key = session_chat_notice_key(project_id, session_id);
    let generation = map
        .get(&key)
        .map(|entry| entry.generation.clone())
        .unwrap_or_else(|| Arc::new(AtomicU64::new(0)));
    let my_generation = generation.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(previous) = map.remove(&key) {
        previous.task.abort();
    }
    let task = spawn(generation.clone(), my_generation);
    map.insert(key, SessionChatWatchdogEntry { generation, task });
}

// ---------------------------------------------------------------------------
// The watchdog task
// ---------------------------------------------------------------------------

struct WatchdogCursor {
    path: Option<PathBuf>,
    /// Where the send happened; the raw scan always re-reads from here.
    base_offset: u64,
    /*
    CDXC:SessionChatTerminalNotices 2026-08-24:
    Whether `base_offset` really marks THIS send. The mismatch tier below reads
    every user turn past it as "recorded after the message was typed", so a
    baseline that was never sampled (the transcript resolved only later) or that
    was reset when the file was rewritten under us would make the whole file
    look post-send and turn a session's own history into a false alarm. The
    delivery tiers do not care — finding the sent text anywhere still proves
    delivery — so this gates only the mismatch scan.
    */
    baseline_trusted: bool,
    state: SessionChatIncrementalState,
    /*
    Sticky affirmative evidence, monotone once set: the first post-send user
    turn that is not the message we sent (`true` ⇒ it was empty), and whether
    the agent produced output after it. Re-derived from the whole window on
    every poll, so ordering is never carried across polls; only the verdict is.
    */
    mismatched_input: Option<bool>,
    agent_answered_mismatch: bool,
}

async fn run_session_chat_send_watchdog(
    probe: Arc<SessionChatSendProbe>,
    publish: SessionChatWatchdogPublisher,
    read_state: SessionChatWatchdogStateReader,
    generation: Arc<AtomicU64>,
    my_generation: u64,
) {
    let superseded = || generation.load(Ordering::SeqCst) != my_generation;
    if superseded() {
        return;
    }
    // A new send is itself proof the session took input, so the previous
    // watchdog's verdict is stale the moment this one starts.
    if clear_session_chat_watchdog_notice(&probe.project_id, &probe.session_id).is_some() {
        publish();
    }

    let decoder = session_chat_line_decoder(probe.transcript_agent);
    let needle = normalize_watchdog_text(&probe.text);
    /*
    Everything the raw scan may look for: what the user typed, plus whatever
    record the CLI writes when it intercepts the input instead of sending it.
    */
    let raw_needles: Vec<String> = json_escaped_needle(&probe.text)
        .into_iter()
        .chain(
            probe
                .intercepted
                .iter()
                .flat_map(InterceptedInput::delivery_needles),
        )
        .collect();
    let mut cursor = WatchdogCursor {
        path: probe.transcript_path.clone(),
        base_offset: probe.transcript_offset,
        baseline_trusted: probe.transcript_path.is_some(),
        state: SessionChatIncrementalState::new(),
        mismatched_input: None,
        agent_answered_mismatch: false,
    };
    cursor.state.rebase(probe.transcript_offset);

    let started = Instant::now();
    // Polls observed since the affirmative evidence became complete.
    let mut mismatch_polls = 0u32;
    loop {
        tokio::time::sleep(WATCHDOG_POLL_INTERVAL).await;
        if superseded() {
            return;
        }
        let poll_probe = probe.clone();
        let poll_needle = needle.clone();
        let poll_raw_needles = raw_needles.clone();
        let Ok((delivered, returned)) = tokio::task::spawn_blocking(move || {
            let mut cursor = cursor;
            let delivered = poll_transcript_for_send(
                &poll_probe,
                &mut cursor,
                decoder,
                &poll_needle,
                &poll_raw_needles,
            );
            (delivered, cursor)
        })
        .await
        else {
            return;
        };
        cursor = returned;
        if delivered {
            if superseded() {
                return;
            }
            if clear_session_chat_watchdog_notice(&probe.project_id, &probe.session_id).is_some() {
                publish();
            }
            return;
        }
        // Affirmative non-delivery: the composer was submitted past our message
        // and the agent is answering what it submitted instead. Nothing the
        // remaining deadline could observe would change that verdict.
        if cursor.mismatched_input.is_some() && cursor.agent_answered_mismatch {
            if mismatch_polls >= WATCHDOG_MISMATCH_GRACE_POLLS {
                break;
            }
            mismatch_polls += 1;
        }
        if started.elapsed() >= WATCHDOG_DEADLINE {
            break;
        }
    }

    if superseded() {
        return;
    }
    let reason = match cursor.mismatched_input {
        Some(submitted_empty) => UndeliveredSendReason::MismatchedInput { submitted_empty },
        None => UndeliveredSendReason::TranscriptSilent,
    };
    escalate_undelivered_send(&probe, cursor.path.is_some(), &publish, &read_state, reason).await;
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Both escalations take the same single capture and the same verdict order; they
differ in how much is already known, and the suppressions below exist only for
the half that is reasoning from silence.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
enum UndeliveredSendReason {
    /// The terminal took the message, but nothing appeared in the transcript
    /// before the deadline. Delivery is UNPROVEN, so a plausible innocent
    /// explanation (a client-side queue, a turn still running, no transcript to
    /// watch at all) has to win over the alarm.
    TranscriptSilent,
    /*
    CDXC:SessionChatTerminalNotices 2026-08-24:
    The terminal took the message and then recorded a DIFFERENT user turn —
    usually an empty one, submitted by the send's trailing Enter before the
    paste had been ingested. Non-delivery is evidenced rather than inferred, so
    the "already working, still working" suppression must not apply: the turn it
    points at is the one this mismatched input started.
    */
    MismatchedInput {
        submitted_empty: bool,
    },
    /// The send itself failed: the message was never typed into the terminal.
    /// Non-delivery is a FACT here, so there is nothing to suppress against and
    /// exactly one of the three verdicts is always published.
    WriteFailed,
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
The message is undelivered — the deadline passed with nothing in the transcript,
the transcript recorded a different prompt in its place, or the send itself
failed (see `UndeliveredSendReason`). Exactly one
terminal capture happens here — never in the poll loop — and the verdict is
decided in suppression-first order, because a false "your message was lost" is
worse than a missed one:
  1. Codex queued the input client-side (nothing is written until the running
     turn ends) — say so, at severity info. (Silence only.)
  2. The screen explains itself (login expired, trust dialog, ...) — publish THAT
     notice so the client renders the specific card, re-sourced to the watchdog.
     Skipped for input the CLI intercepted: the screen then belongs to the
     command that just ran (`/model`'s picker, `/usage`'s quota view), and
     narrating it as "what the terminal is showing INSTEAD of your message" is
     wrong twice over. A genuinely blocking screen is still published by the
     screen detector on the next read, which does not need the watchdog.
  3. Clean screen: consult Claude's own session registry before blaming the
     process.
  4. The agent is working at the deadline — normal, stay silent: either the
     turn that was already running is holding the input queued, or the send
     itself started the turn. (Silence only, and NOT for `MismatchedInput`:
     there the running turn is the one the mismatched input started, so reading
     it as an innocent explanation is exactly what swallowed this alarm before.)
  5. Otherwise the honest verdict — after one last-chance scan of freshly
     re-resolved transcript candidates, which clears a send whose watched file
     rotated out from under the watchdog: nothing proves the message arrived
     (a WARNING suggestion, not an error) — or, for `MismatchedInput`, positive
     proof that something else was submitted.
*/
async fn escalate_undelivered_send(
    probe: &SessionChatSendProbe,
    transcript_watched: bool,
    publish: &SessionChatWatchdogPublisher,
    read_state: &SessionChatWatchdogStateReader,
    reason: UndeliveredSendReason,
) {
    /*
    Two different questions, deliberately not one flag:
      - `typed_into_terminal` — the message DID reach the terminal, so an
        innocent explanation (a client-side queue, a command the CLI ran itself,
        a session with no transcript yet) can still outrank the alarm. A send
        that failed has none of those.
      - `reasoning_from_silence` — the verdict rests on nothing having been
        recorded. `MismatchedInput` does not: it recorded the wrong thing.
    */
    let typed_into_terminal = reason != UndeliveredSendReason::WriteFailed;
    let reasoning_from_silence = reason == UndeliveredSendReason::TranscriptSilent;
    let screen = crate::session_chat_send::capture_session_terminal_text(&probe.zmx_name).await;
    if let Some(screen) = screen.as_deref().filter(|_| typed_into_terminal) {
        // A queue banner explains a silent transcript; it explains nothing about
        // a message the terminal never accepted.
        if session_chat_screen_shows_queued_input(probe.agent.as_deref(), screen) {
            publish_watchdog_notice(
                probe,
                SessionChatTerminalNotice::new(
                    SESSION_CHAT_NOTICE_QUEUED_INPUT,
                    SessionChatTerminalNoticeSeverity::Info,
                    SessionChatTerminalNoticeSource::Watchdog,
                    "Message queued behind the current turn",
                )
                .with_detail(
                    "The agent is holding your message until the turn it is running finishes, so it has not been sent to the model yet.",
                )
                .with_screen_tail(session_chat_terminal_screen_tail(screen))
                .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
                    "Open terminal",
                )]),
                publish,
            );
            return;
        }
    }

    let screen_tail = screen
        .as_deref()
        .and_then(session_chat_terminal_screen_tail);
    /*
    A blocking screen outranks the still-working suppression below. Hook
    activity is a claim about a turn that STARTED; when the CLI dies or blocks
    mid-turn nothing ever clears it, and that stuck "working" is exactly the
    state this feature exists to explain.
    */
    let screen_explains_the_send = !(typed_into_terminal && probe.intercepted.is_some());
    if let Some(screen) = screen.as_deref().filter(|_| screen_explains_the_send) {
        if let Some(mut notice) =
            classify_session_chat_terminal_notice(probe.agent.as_deref(), screen)
        {
            notice.source = SessionChatTerminalNoticeSource::Watchdog;
            let prefix = undelivered_prefix(reason);
            notice.detail = Some(match notice.detail {
                Some(detail) => format!("{prefix} {detail}"),
                None => prefix.to_string(),
            });
            publish_watchdog_notice(probe, notice, publish);
            return;
        }
    }

    let reader = read_state.clone();
    let live = tokio::task::spawn_blocking(move || reader())
        .await
        .unwrap_or_default();
    if live.running
        && probe_claude_agent_liveness(probe.agent.as_deref(), probe.agent_session_id.as_deref())
            == ClaudeAgentLiveness::Exited
    {
        publish_watchdog_notice(
            probe,
            SessionChatTerminalNotice::new(
                SESSION_CHAT_NOTICE_AGENT_EXITED,
                SessionChatTerminalNoticeSeverity::Error,
                SessionChatTerminalNoticeSource::Watchdog,
                "Claude Code is no longer running in this terminal",
            )
            .with_detail(match reason {
                UndeliveredSendReason::TranscriptSilent => "Your message was never recorded, and the Claude Code process that owned this session is no longer registered as running — it appears to have exited. Start it again in the terminal before sending more messages.",
                UndeliveredSendReason::MismatchedInput { .. } => "Your message was not recorded — a different prompt was submitted in its place — and the Claude Code process that owned this session is no longer registered as running. Start it again in the terminal before sending more messages.",
                UndeliveredSendReason::WriteFailed => "Your message could not be typed into this session, and the Claude Code process that owned it is no longer registered as running — it appears to have exited. Start it again in the terminal before sending more messages.",
            })
            .with_screen_tail(screen_tail)
            .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
                "Open terminal",
            )]),
            publish,
        );
        return;
    }

    if typed_into_terminal {
        /*
        The agent is working at the deadline, with a clean screen and a live
        process: the silent transcript has an innocent explanation either way.
        A turn that was already running when the message was typed is holding
        the input queued behind it (the Codex queue banner above is the same
        story with a visible witness); a turn that STARTED after the send is
        the send itself being processed, because the only submission recorded
        since the send is ours-or-nothing — a different one would have produced
        `MismatchedInput` above. The started-after case used to alarm (the old
        rule also required hook activity at send time), which fired falsely
        whenever the watched transcript file had gone stale under the watchdog.

        CDXC:SessionChatTerminalNotices 2026-08-24: `MismatchedInput` is
        excluded, and that exclusion is the whole fix. There the transcript
        recorded a user turn AFTER the send that is not ours and the agent went
        to work on it, so "still working" describes the turn that ATE the send.
        Treating it as an innocent explanation is what let an empty submit lose
        a message with no notice at all.
        */
        if reasoning_from_silence && live.working {
            return;
        }
        /*
        The generic verdict is "nothing was recorded", so it needs a transcript
        to have been recorded INTO. A session whose transcript never resolved
        (the agent has not reported its session id yet) gives no evidence either
        way, and guessing there would fire on every healthy first message.
        Positive evidence — a blocking screen, a dead process — still alarms
        above. Neither suppression applies to a send that FAILED: there the
        message provably never left Ghostex.
        */
        if !transcript_watched {
            return;
        }
        /*
        CDXC:SessionChatTerminalNotices 2026-08-20:
        The CLI executes this input itself rather than sending it to the model,
        so a silent transcript is its NORMAL outcome — `/usage`, `/model`,
        `!ls` and the rest write nothing a watcher can see, and the records that
        do exist are matched as delivery above. Reasoning from silence here only
        ever produced a false "your message did not reach the agent" for a
        command that plainly ran. Hard evidence above — an exited CLI — still
        alarms, and a send that FAILED never reaches this suppression.
        */
        if probe.intercepted.is_some() {
            return;
        }
        /*
        CDXC:SessionChatTerminalNotices 2026-08-28:
        Last chance before alarming from silence: re-resolve the transcript and
        scan the tail of every candidate file written since the send. The poll
        loop watches ONE path resolved at send time, and that path can go stale
        under it — Codex rotates its rollout, a recorded agentSessionPath
        outlives the file it named — after which every poll reads a file the
        agent no longer writes and a perfectly delivered message looks silent.
        */
        if reasoning_from_silence && delivered_elsewhere_at_deadline(probe).await {
            return;
        }
    }

    let notice = match reason {
        // The affirmative verdict has its own card, built in the notice catalog
        // because it is the one that tells the user where their text went.
        UndeliveredSendReason::MismatchedInput { submitted_empty } => {
            session_chat_delivery_mismatch_notice(submitted_empty, screen_tail)
        }
        /*
        CDXC:SessionChatTerminalNotices 2026-08-28: this verdict rests on
        NOT having observed something, so it is a suggestion, not a failure:
        a yellow "go make sure" card, never a red alarm. The red card is
        reserved for the two verdicts above/below with affirmative evidence.
        */
        UndeliveredSendReason::TranscriptSilent => SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_DELIVERY_FAILED,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Watchdog,
            "Your message might not have reached the agent",
        )
        .with_detail("Nothing was recorded in the session transcript in the 10 seconds after this message was sent, so delivery could not be confirmed. Open the terminal to make sure it arrived.")
        .with_screen_tail(screen_tail)
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Open terminal",
        )]),
        UndeliveredSendReason::WriteFailed => SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_DELIVERY_FAILED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Watchdog,
            "Your message could not be sent to the agent",
        )
        .with_detail("This session's terminal did not respond while the message was being typed into it, so the message was never delivered to the agent. Open the terminal to see what it is showing.")
        .with_screen_tail(screen_tail)
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Open terminal",
        )]),
    };
    publish_watchdog_notice(probe, notice, publish);
}

/// Leads the detail of a classified screen notice with what the send did.
fn undelivered_prefix(reason: UndeliveredSendReason) -> &'static str {
    match reason {
        UndeliveredSendReason::TranscriptSilent => {
            "Your message was not recorded by the agent — this is what its terminal is showing instead."
        }
        UndeliveredSendReason::MismatchedInput { .. } => {
            "Your message was not delivered — the agent recorded a different prompt in its place — and this is what its terminal is showing."
        }
        UndeliveredSendReason::WriteFailed => {
            "Your message could not be typed into this session's terminal — this is what it is showing instead."
        }
    }
}

/*
CDXC:SessionChatTerminalNotices 2026-08-28:
Deadline-time delivery proof that does not trust the send-time path. Resolution
runs twice — once as the send did (recorded path first) and once by session id
alone — so a stale recorded path cannot mask the live file the id sweep finds.
Every candidate written since the send gets its tail scanned for the message,
both as raw JSON-escaped bytes and as decoded composer submissions (which is
what covers a send too short for the raw scan). A hit means the message IS in
a transcript the agent is writing, so the alarm would be a lie; the residual
risk — an identical earlier message in the same tail masking a real loss — is
the cheap side of "a false 'your message was lost' is worse than a missed one".
*/
async fn delivered_elsewhere_at_deadline(probe: &SessionChatSendProbe) -> bool {
    let transcript_agent = probe.transcript_agent;
    let agent_session_id = probe.agent_session_id.clone();
    let agent_session_path = probe.agent_session_path.clone();
    let text = probe.text.clone();
    let sent_at = probe.sent_at;
    tokio::task::spawn_blocking(move || {
        transcript_tail_proves_delivery(
            transcript_agent,
            agent_session_id.as_deref(),
            agent_session_path.as_deref(),
            &text,
            sent_at,
        )
    })
    .await
    .unwrap_or(false)
}

/// BLOCKING (filesystem walk + bounded tail reads).
fn transcript_tail_proves_delivery(
    agent: SessionChatTranscriptAgent,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
    text: &str,
    sent_at: std::time::SystemTime,
) -> bool {
    let needle = normalize_watchdog_text(text);
    if needle.is_empty() {
        return false;
    }
    let raw_needles: Vec<String> = json_escaped_needle(text).into_iter().collect();
    let mut candidates: Vec<PathBuf> = Vec::new();
    for path in [
        resolve_session_chat_transcript_path(agent, agent_session_id, agent_session_path),
        // Session-id sweep with the recorded path ignored: this is the lookup
        // that finds the newest live file after a rotation.
        resolve_session_chat_transcript_path(agent, agent_session_id, None),
    ]
    .into_iter()
    .flatten()
    {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    for path in candidates {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        // Only a file the agent wrote AFTER the send can testify about it.
        if !metadata
            .modified()
            .is_ok_and(|modified| modified >= sent_at)
        {
            continue;
        }
        let offset = metadata.len().saturating_sub(WATCHDOG_RAW_SCAN_LIMIT_BYTES);
        let Some(tail) = read_appended_text(&path, offset, WATCHDOG_RAW_SCAN_LIMIT_BYTES) else {
            continue;
        };
        if raw_needles.iter().any(|raw| tail.contains(raw)) {
            return true;
        }
        // The first line of a mid-file tail is truncated; its parse just fails.
        for line in tail.lines() {
            if let Some(WatchdogRecord::UserSubmission(submitted)) =
                classify_watchdog_record(agent, line)
            {
                if watchdog_text_matches(&submitted, &needle) {
                    return true;
                }
            }
        }
    }
    false
}

/// Stores the verdict and pushes a frame only when it actually says something
/// new: a repeat notice must keep its original `detectedAt`, which is the
/// client's dismissal key.
fn publish_watchdog_notice(
    probe: &SessionChatSendProbe,
    notice: SessionChatTerminalNotice,
    publish: &SessionChatWatchdogPublisher,
) {
    let previous = session_chat_watchdog_notice(&probe.project_id, &probe.session_id);
    if notice.same_notice(previous.as_ref()) {
        return;
    }
    set_session_chat_watchdog_notice(&probe.project_id, &probe.session_id, notice);
    publish();
}

// ---------------------------------------------------------------------------
// Delivery verification
// ---------------------------------------------------------------------------

/// BLOCKING (filesystem). One tick of the two-tier check over everything the
/// transcript gained since the send.
fn poll_transcript_for_send(
    probe: &SessionChatSendProbe,
    cursor: &mut WatchdogCursor,
    decoder: SessionChatLineDecoder,
    needle: &str,
    raw_needles: &[String],
) -> bool {
    if cursor.path.is_none() {
        // Codex creates the rollout lazily, so a brand-new session has no file
        // to sample at send time; everything in it is post-send by definition.
        cursor.path = resolve_session_chat_transcript_path(
            probe.transcript_agent,
            probe.agent_session_id.as_deref(),
            probe.agent_session_path.as_deref(),
        );
        if cursor.path.is_some() {
            cursor.base_offset = 0;
            cursor.state.rebase(0);
            // A file that only appeared now was never sampled at send time, so
            // "everything past the baseline is post-send" is an assumption, not
            // a measurement. Good enough to look for the sent text in; not good
            // enough to read other people's turns as evidence against it.
            cursor.baseline_trusted = false;
        }
    }
    let Some(path) = cursor.path.clone() else {
        return false;
    };
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    if metadata.len() < cursor.state.offset {
        // The file was replaced or rewritten under us; re-read it whole rather
        // than tailing an offset that no longer exists. The send's baseline is
        // gone with it, so the mismatch scan stops trusting it.
        cursor.base_offset = 0;
        cursor.state.rebase(0);
        cursor.baseline_trusted = false;
    }

    let mut matched = false;
    let decoded = {
        let mut on_batch = |batch: Vec<SessionChatMessage>| {
            if !matched {
                matched = batch
                    .iter()
                    .any(|message| user_message_matches(message, needle));
            }
        };
        read_incremental_transcript_messages(
            &path,
            &mut cursor.state,
            decoder,
            Some(&mut on_batch),
            None,
            None,
            None,
        )
    };
    if matched {
        return true;
    }
    if let Ok(messages) = decoded {
        if messages
            .iter()
            .any(|message| user_message_matches(message, needle))
        {
            return true;
        }
    }

    /*
    Second tier: the decoders skip rows that still prove delivery — Claude's
    `queue-operation` enqueue record (typed while a turn ran) and Codex's
    `response_item` message lane. Both carry the text verbatim inside a JSON
    string, so the needle is escaped the same way before the scan. Input the CLI
    intercepted adds its own shapes to look for: it is never a user turn, but
    Claude does record `<command-name>` / `<bash-input>` rows naming it.
    */
    /*
    The same appended window answers the mismatch question below, so it is read
    once. Skipping the read entirely when neither tier can use it keeps a short
    send on a session with no usable baseline at zero filesystem cost.
    */
    let scan_for_mismatch = cursor.baseline_trusted && !cursor.agent_answered_mismatch;
    if raw_needles.is_empty() && !scan_for_mismatch {
        return false;
    }
    let Some(appended) =
        read_appended_text(&path, cursor.base_offset, WATCHDOG_RAW_SCAN_LIMIT_BYTES)
    else {
        return false;
    };
    if raw_needles
        .iter()
        .any(|raw_needle| appended.contains(raw_needle))
    {
        return true;
    }
    if scan_for_mismatch {
        observe_mismatched_input(probe.transcript_agent, cursor, &appended, needle);
    }
    false
}

fn user_message_matches(message: &SessionChatMessage, needle: &str) -> bool {
    message.role == SessionChatRole::User
        && watchdog_text_matches(&message_plain_text(message), needle)
}

/// Whitespace-folded containment, or a prefix that carries most of the send.
fn watchdog_text_matches(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let text = normalize_watchdog_text(text);
    if text.is_empty() {
        return false;
    }
    if text.contains(needle) {
        return true;
    }
    needle.starts_with(&text)
        && text.chars().count() * 100 >= needle.chars().count() * WATCHDOG_PREFIX_MATCH_PERCENT
}

fn message_plain_text(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Agent TUIs re-wrap and re-indent what the user typed, so both sides of the
/// comparison are reduced to single-spaced words.
fn normalize_watchdog_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Mismatched-input evidence
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatTerminalNotices 2026-08-24:
The tiers above answer "did our message arrive?" and can only ever fail to prove
it. This one answers the sharper question "was something ELSE submitted in its
place?", which is what actually happened in the incident this exists for: the
send's trailing Enter committed the composer before the pasted text had been
ingested into it, so the agent recorded an EMPTY user turn, started answering it,
and the message stayed behind in the composer until another terminal write
replaced or cleared it.

It cannot reuse the decoders. Both of them drop a contentless user row on the
floor (`claude_content_blocks` returns no blocks for an empty string, and
`extract_string` discards empty text), which is precisely the row that proves
the failure — so this reads the appended records itself, and deliberately reads
only the two lanes that mean "the human's composer submitted this": Claude's
`user` rows and Codex's event lanes. Harness-injected user turns, tool results,
queue rows and Codex's envelope-carrying `response_item` message twin are all
excluded, because none of them is something a person submitted.
*/
enum WatchdogRecord {
    /// A composer submission, with its plain text — EMPTY when a bare Enter
    /// submitted nothing.
    UserSubmission(String),
    /// The agent produced output of its own: a turn is under way.
    AgentOutput,
}

/// BLOCKING-free: scans the already-read window. Re-derived from the whole
/// window every poll so record ORDER is never carried across polls; only the
/// verdict is, and only ever in one direction.
fn observe_mismatched_input(
    agent: SessionChatTranscriptAgent,
    cursor: &mut WatchdogCursor,
    appended: &str,
    needle: &str,
) {
    let mut mismatch: Option<bool> = None;
    let mut agent_answered = false;
    for line in appended.lines() {
        match classify_watchdog_record(agent, line) {
            Some(WatchdogRecord::UserSubmission(text)) => {
                if mismatch.is_none() && !watchdog_text_matches(&text, needle) {
                    mismatch = Some(text.trim().is_empty());
                }
            }
            // Only output that follows the mismatched turn says the agent went
            // to work on it; the tail of the turn our send was typed into does
            // not.
            Some(WatchdogRecord::AgentOutput) => agent_answered |= mismatch.is_some(),
            None => {}
        }
    }
    let Some(submitted_empty) = mismatch else {
        return;
    };
    cursor.mismatched_input.get_or_insert(submitted_empty);
    cursor.agent_answered_mismatch |= agent_answered;
}

fn classify_watchdog_record(
    agent: SessionChatTranscriptAgent,
    line: &str,
) -> Option<WatchdogRecord> {
    let record = parse_json_object(line)?;
    match agent {
        SessionChatTranscriptAgent::Claude => claude_watchdog_record(&record),
        SessionChatTranscriptAgent::Codex => codex_watchdog_record(&record),
        // No catalogued record shapes, so no evidence either way. The delivery
        // tiers and the 10s deadline still cover these agents unchanged.
        SessionChatTranscriptAgent::Antigravity
        | SessionChatTranscriptAgent::Grok
        | SessionChatTranscriptAgent::Cursor
        | SessionChatTranscriptAgent::Hermes
        | SessionChatTranscriptAgent::Pi => None,
    }
}

fn claude_watchdog_record(record: &Map<String, Value>) -> Option<WatchdogRecord> {
    match record.get("type").and_then(Value::as_str)? {
        "assistant" => Some(WatchdogRecord::AgentOutput),
        "user" => {
            // Harness plumbing wearing the user role: replayed summaries, the
            // injected meta turns, and the marker row an interrupt writes.
            if record.get("isMeta") == Some(&Value::Bool(true))
                || record.get("isSynthetic") == Some(&Value::Bool(true))
                || record.get("isCompactSummary") == Some(&Value::Bool(true))
                || record.contains_key("interruptedMessageId")
            {
                return None;
            }
            let message = record.get("message")?.as_object()?;
            let text = claude_submitted_text(message.get("content")?)?;
            user_submission_record(text)
        }
        // `queue-operation` and `attachment` rows describe a prompt the CLI is
        // HOLDING, not one it submitted past ours, and the raw needle tier
        // already reads them as delivery proof.
        _ => None,
    }
}

/// The text a Claude `user` row submitted. `None` for content that is not a
/// composer submission at all (tool results, images, attachments).
fn claude_submitted_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut parts: Vec<&str> = Vec::new();
            for item in items {
                match item {
                    Value::String(text) => parts.push(text),
                    Value::Object(block) => match block.get("type").and_then(Value::as_str) {
                        Some("text") => parts.push(
                            block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        ),
                        _ => return None,
                    },
                    _ => return None,
                }
            }
            Some(parts.join(" "))
        }
        _ => None,
    }
}

fn codex_watchdog_record(record: &Map<String, Value>) -> Option<WatchdogRecord> {
    let payload = record.get("payload")?.as_object()?;
    let payload_type = payload.get("type").and_then(Value::as_str)?;
    match record.get("type").and_then(Value::as_str)? {
        "event_msg" => match payload_type {
            "user_message" => {
                user_submission_record(payload.get("message").and_then(Value::as_str)?.to_string())
            }
            "agent_message" | "task_started" => Some(WatchdogRecord::AgentOutput),
            "item_completed" => {
                let item = payload.get("item")?.as_object()?;
                match item.get("type").and_then(Value::as_str)? {
                    "UserMessage" => {
                        user_submission_record(codex_item_submitted_text(item.get("content")?)?)
                    }
                    "AgentMessage" => Some(WatchdogRecord::AgentOutput),
                    _ => None,
                }
            }
            _ => None,
        },
        /*
        The `response_item` message lane is the envelope-carrying twin of the
        event lane (see `codex_response_item`), so its user rows are NOT
        submissions; its assistant and tool lanes are still the agent working.
        */
        "response_item" => match payload_type {
            "reasoning" | "function_call" | "local_shell_call" | "custom_tool_call"
            | "web_search_call" | "tool_search_call" => Some(WatchdogRecord::AgentOutput),
            "message" => (payload.get("role").and_then(Value::as_str) == Some("assistant"))
                .then_some(WatchdogRecord::AgentOutput),
            _ => None,
        },
        _ => None,
    }
}

/// `item_completed` UserMessage content, which spells its text block `text`.
fn codex_item_submitted_text(content: &Value) -> Option<String> {
    let items = content.as_array()?;
    let mut parts: Vec<&str> = Vec::new();
    for item in items {
        let block = item.as_object()?;
        match block.get("type").and_then(Value::as_str) {
            Some("text" | "Text" | "input_text") => parts.push(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            // The slash-skill chip carries no typed text of its own.
            Some("skill") => {}
            _ => return None,
        }
    }
    Some(parts.join(" "))
}

/// Wraps submitted text as evidence, minus the harness envelopes that ride the
/// user role. An EMPTY submission is kept: it is the whole point of this tier.
fn user_submission_record(text: String) -> Option<WatchdogRecord> {
    let probe = SessionChatMessage {
        id: String::new(),
        role: SessionChatRole::User,
        blocks: vec![text_block(text.clone())],
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
    };
    (!is_noise_message(&probe)).then_some(WatchdogRecord::UserSubmission(text))
}

/// The needle as it appears INSIDE a JSON string on disk (no surrounding
/// quotes). `None` when the text is too short for a substring scan to be
/// evidence. Non-ASCII text that the agent wrote with `\uXXXX` escapes will not
/// match here — that is what the decoded tier is for.
fn json_escaped_needle(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.chars().count() < WATCHDOG_RAW_SCAN_MIN_CHARS {
        return None;
    }
    let encoded = serde_json::to_string(trimmed).ok()?;
    let inner = encoded.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.is_empty()).then(|| inner.to_string())
}

/*
CDXC:SessionChatTerminalNotices 2026-08-20:
A message the CLI executes itself instead of sending to the model. Both agents
have exactly two of these, and both use the same two prefixes:
  - `/command` — Claude's local commands and Codex's `SlashCommand` popup, from
    `/usage` and `/model` (pure UI, nothing recorded anywhere) through `/init`
    and `/compact` (a turn is recorded, but its text is the command's expanded
    prompt, never what the user typed);
  - `!command` — a shell escape in both CLIs, run locally and never sent.
Recognition is deliberately strict, because everything downstream of it either
adds evidence or REMOVES an alarm: a single line, an alphabetic first character
after the prefix, and no other punctuation inside the name, so a pasted path
(`/Users/...`) or a prose message that opens with a slash stays an ordinary
message. Namespaced plugin commands (`/plugin-dev:agent-creator`) keep their
colon — the CLI logs the full name and the needle below has to match it.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
enum InterceptedInput {
    /// The `/command` token, prefix included.
    SlashCommand(String),
    /// The command body with the `!` stripped, which is how both CLIs echo and
    /// (for Claude) record it.
    ShellEscape(String),
}

impl InterceptedInput {
    fn detect(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        if trimmed.lines().count() != 1 {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix('/') {
            let mut name = String::new();
            for ch in rest.chars() {
                if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':') {
                    name.push(ch);
                    continue;
                }
                if ch.is_whitespace() {
                    break;
                }
                return None;
            }
            if !name.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                return None;
            }
            return Some(Self::SlashCommand(format!("/{name}")));
        }
        let command = trimmed.strip_prefix('!')?.trim();
        if !command.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            return None;
        }
        Some(Self::ShellEscape(command.to_string()))
    }

    /// Extra raw-scan needles that PROVE the CLI took the input. Claude names
    /// the command in a record of its own; Codex contributes nothing here,
    /// which is what the suppression exists for.
    fn delivery_needles(&self) -> Vec<String> {
        match self {
            // Both the tag and the command name are JSON-escape-free, so this
            // matches the raw transcript bytes exactly as written.
            Self::SlashCommand(command) => {
                vec![format!("<command-name>{command}</command-name>")]
            }
            // `<bash-input>` carries the command without its `!`, so the typed
            // text never matches it; the stripped body does.
            Self::ShellEscape(command) => json_escaped_needle(command).into_iter().collect(),
        }
    }
}

fn read_appended_text(path: &Path, offset: u64, limit: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buffer: Vec<u8> = Vec::new();
    file.take(limit).read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

// ---------------------------------------------------------------------------
// Claude liveness evidence
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaudeAgentLiveness {
    Alive,
    Exited,
    /// No registry to read, so the question was never answered — say nothing.
    Unknown,
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Every live Claude CLI keeps `~/.claude/sessions/<pid>.json` describing itself
(pid, sessionId, cwd, status). gxserver knows the session's agentSessionId, so a
matching record whose pid is gone — or no record at all on a machine that keeps
this registry — is the only hard evidence the server can get that the agent
process itself died, since zmx only knows whether the PANE exists.

`updatedAt` is deliberately NOT used as the freshness test: it is written on
status changes, not as a heartbeat, so a busy Claude routinely shows minutes of
"staleness" and a time-based rule would invent exits. The pid is the truth.

Bounded on purpose: read only at watchdog escalation, never in the fingerprint
loop or a frame path.
*/
fn probe_claude_agent_liveness(
    agent: Option<&str>,
    agent_session_id: Option<&str>,
) -> ClaudeAgentLiveness {
    if !matches!(agent.map(str::trim), Some("claude" | "openclaude")) {
        return ClaudeAgentLiveness::Unknown;
    }
    let Some(agent_session_id) = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ClaudeAgentLiveness::Unknown;
    };
    let mut registry_seen = false;
    let mut scanned = 0usize;
    for directory in claude_session_registry_dirs() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if scanned >= CLAUDE_REGISTRY_SCAN_LIMIT {
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            scanned += 1;
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(record) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let Some(pid) = record.get("pid").and_then(Value::as_u64) else {
                continue;
            };
            registry_seen = true;
            if record.get("sessionId").and_then(Value::as_str) != Some(agent_session_id) {
                continue;
            }
            return if claude_registry_process_alive(pid) {
                ClaudeAgentLiveness::Alive
            } else {
                ClaudeAgentLiveness::Exited
            };
        }
    }
    /*
    No record for our session. That only means "exited" for stock `claude`,
    whose registry we just proved this machine keeps: an `openclaude` fork may
    simply not write one, and the neighbouring stock entries would then frame it
    for an exit it never had.
    */
    if registry_seen && agent.map(str::trim) == Some("claude") {
        ClaudeAgentLiveness::Exited
    } else {
        ClaudeAgentLiveness::Unknown
    }
}

/// Mirrors `resume_lookup::claude_project_roots`: the default home plus every
/// `~/.claude-profiles/<profile>` Ghostex may have launched the agent under.
fn claude_session_registry_dirs() -> Vec<PathBuf> {
    let home = crate::resume_lookup::home_dir();
    let mut directories = vec![home.join(".claude").join("sessions")];
    if let Ok(profiles) = std::fs::read_dir(home.join(".claude-profiles")) {
        for profile in profiles.flatten() {
            directories.push(profile.path().join("sessions"));
        }
    }
    directories
}

fn claude_registry_process_alive(pid: u64) -> bool {
    #[cfg(unix)]
    {
        u32::try_from(pid).is_ok_and(crate::runtime::is_process_running)
    }
    // Nothing to check the pid against, so never claim the process is gone.
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}
