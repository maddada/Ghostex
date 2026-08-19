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

use serde_json::Value;

use crate::session_chat::{
    read_incremental_transcript_messages, resolve_session_chat_transcript_agent,
    resolve_session_chat_transcript_path, session_chat_line_decoder, SessionChatBlock,
    SessionChatIncrementalState, SessionChatLineDecoder, SessionChatMessage, SessionChatRole,
    SessionChatTranscriptAgent,
};
use crate::session_chat_notice::{
    classify_session_chat_terminal_notice, clear_session_chat_watchdog_notice,
    session_chat_notice_key, session_chat_screen_shows_queued_input,
    session_chat_terminal_screen_tail, session_chat_watchdog_notice,
    set_session_chat_watchdog_notice, SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
    SESSION_CHAT_NOTICE_AGENT_EXITED, SESSION_CHAT_NOTICE_DELIVERY_FAILED,
    SESSION_CHAT_NOTICE_QUEUED_INPUT,
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
    /// Hook activity at send time. A turn that was already running when the
    /// message was typed explains a silent transcript without any failure.
    working_at_send: bool,
    text: String,
}

impl SessionChatSendProbe {
    /// `None` disables the watchdog for this send: only the catalogued agents
    /// (claude/openclaude/codex) have verified transcript-write semantics, and
    /// an image-only send has no text to look for.
    #[allow(clippy::too_many_arguments)]
    pub async fn sample(
        project_id: &str,
        session_id: &str,
        zmx_name: &str,
        agent: Option<&str>,
        agent_session_id: Option<&str>,
        agent_session_path: Option<&str>,
        working_at_send: bool,
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
            working_at_send,
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
    state: SessionChatIncrementalState,
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
    let raw_needle = json_escaped_needle(&probe.text);
    let mut cursor = WatchdogCursor {
        path: probe.transcript_path.clone(),
        base_offset: probe.transcript_offset,
        state: SessionChatIncrementalState::new(),
    };
    cursor.state.rebase(probe.transcript_offset);

    let started = Instant::now();
    loop {
        tokio::time::sleep(WATCHDOG_POLL_INTERVAL).await;
        if superseded() {
            return;
        }
        let poll_probe = probe.clone();
        let poll_needle = needle.clone();
        let poll_raw_needle = raw_needle.clone();
        let Ok((delivered, returned)) = tokio::task::spawn_blocking(move || {
            let mut cursor = cursor;
            let delivered = poll_transcript_for_send(
                &poll_probe,
                &mut cursor,
                decoder,
                &poll_needle,
                poll_raw_needle.as_deref(),
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
        if started.elapsed() >= WATCHDOG_DEADLINE {
            break;
        }
    }

    if superseded() {
        return;
    }
    escalate_undelivered_send(
        &probe,
        cursor.path.is_some(),
        &publish,
        &read_state,
        UndeliveredSendReason::TranscriptSilent,
    )
    .await;
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
    /// The send itself failed: the message was never typed into the terminal.
    /// Non-delivery is a FACT here, so there is nothing to suppress against and
    /// exactly one of the three verdicts is always published.
    WriteFailed,
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
The message is undelivered — either the deadline passed with nothing in the
transcript, or the send itself failed (see `UndeliveredSendReason`). Exactly one
terminal capture happens here — never in the poll loop — and the verdict is
decided in suppression-first order, because a false "your message was lost" is
worse than a missed one:
  1. Codex queued the input client-side (nothing is written until the running
     turn ends) — say so, at severity info. (Silence only.)
  2. The screen explains itself (login expired, trust dialog, ...) — publish THAT
     notice so the client renders the specific card, re-sourced to the watchdog.
  3. Clean screen: consult Claude's own session registry before blaming the
     process.
  4. A turn was already running when the message was typed and is still running
     — normal, stay silent. (Silence only.)
  5. Otherwise the honest verdict: nothing proves the message arrived.
*/
async fn escalate_undelivered_send(
    probe: &SessionChatSendProbe,
    transcript_watched: bool,
    publish: &SessionChatWatchdogPublisher,
    read_state: &SessionChatWatchdogStateReader,
    reason: UndeliveredSendReason,
) {
    let reasoning_from_silence = reason == UndeliveredSendReason::TranscriptSilent;
    let screen = crate::session_chat_send::capture_session_terminal_text(&probe.zmx_name).await;
    if let Some(screen) = screen.as_deref().filter(|_| reasoning_from_silence) {
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
    if let Some(screen) = screen.as_deref() {
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

    if reasoning_from_silence {
        // Typed into a running turn that is still running, with a clean screen
        // and a live agent: the transcript is silent for a reason that is not a
        // failure.
        if probe.working_at_send && live.working {
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
    }

    publish_watchdog_notice(
        probe,
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_DELIVERY_FAILED,
            SessionChatTerminalNoticeSeverity::Error,
            SessionChatTerminalNoticeSource::Watchdog,
            match reason {
                UndeliveredSendReason::TranscriptSilent => "Your message did not reach the agent",
                UndeliveredSendReason::WriteFailed => "Your message could not be sent to the agent",
            },
        )
        .with_detail(match reason {
            UndeliveredSendReason::TranscriptSilent => "Nothing was written to the session transcript in the 10 seconds after this message was sent, so it most likely never reached the agent. Open the terminal to see what it is showing.",
            UndeliveredSendReason::WriteFailed => "This session's terminal did not respond while the message was being typed into it, so the message was never delivered to the agent. Open the terminal to see what it is showing.",
        })
        .with_screen_tail(screen_tail)
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Open terminal",
        )]),
        publish,
    );
}

/// Leads the detail of a classified screen notice with what the send did.
fn undelivered_prefix(reason: UndeliveredSendReason) -> &'static str {
    match reason {
        UndeliveredSendReason::TranscriptSilent => {
            "Your message was not recorded by the agent — this is what its terminal is showing instead."
        }
        UndeliveredSendReason::WriteFailed => {
            "Your message could not be typed into this session's terminal — this is what it is showing instead."
        }
    }
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
    raw_needle: Option<&str>,
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
        // than tailing an offset that no longer exists.
        cursor.base_offset = 0;
        cursor.state.rebase(0);
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
    string, so the needle is escaped the same way before the scan.
    */
    let Some(raw_needle) = raw_needle else {
        return false;
    };
    read_appended_text(&path, cursor.base_offset, WATCHDOG_RAW_SCAN_LIMIT_BYTES)
        .is_some_and(|appended| appended.contains(raw_needle))
}

fn user_message_matches(message: &SessionChatMessage, needle: &str) -> bool {
    if message.role != SessionChatRole::User || needle.is_empty() {
        return false;
    }
    let text = normalize_watchdog_text(&message_plain_text(message));
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
