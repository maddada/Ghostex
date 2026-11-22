use super::*;

/// Poll cadence. The transcript is flushed per line, so a delivered message is
/// normally visible on the first tick.
pub(super) const WATCHDOG_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Alarm budget. Claude records the user turn in well under a second; Codex
/// writes it before it even issues the model request. 10s is slack, not hope.
pub(super) const WATCHDOG_DEADLINE: Duration = Duration::from_secs(10);
/// The raw scan keeps the OLDEST appended bytes: the record we are looking for
/// is written first, and a chatty turn can append megabytes behind it.
pub(super) const WATCHDOG_RAW_SCAN_LIMIT_BYTES: u64 = 512 * 1024;
/// Below this length a raw substring scan stops being evidence — "ok" appears
/// in everything. Short sends still match through the decoded tier.
pub(super) const WATCHDOG_RAW_SCAN_MIN_CHARS: usize = 12;
/// A decoded turn that carries most of what was sent counts as delivered: TUIs
/// trim and re-wrap, and a partial match still proves the message landed.
pub(super) const WATCHDOG_PREFIX_MATCH_PERCENT: usize = 80;
/*
CDXC:AgentScreenDetection 2026-08-24:
Once the transcript has recorded a user turn that is NOT ours AND the agent has
started answering it, waiting out the rest of the deadline buys nothing: the
composer has already been submitted past our message. These extra polls exist
only so a transcript that writes the agent's rows before the user row it is
answering cannot be read as a mismatch.
*/
pub(super) const WATCHDOG_MISMATCH_GRACE_POLLS: u32 = 2;
/// Registry files are one per live CLI; a machine has a handful, not thousands.
pub(super) const CLAUDE_REGISTRY_SCAN_LIMIT: usize = 256;

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

/// Starts the returned-prompt detector for this send (supplied by the send
/// path, which owns the AppState the detector needs). Called at the deadline
/// instead of a delivery verdict when the screen shows the sent text back in
/// Claude's composer (CDXC:SessionChat in session_chat_returned_prompt.rs).
pub type SessionChatReturnedPromptTrigger = Arc<dyn Fn() + Send + Sync>;

/// Restarts a session whose agent exited under a send and holds the message for the new agent
/// (session_chat_queue_runtime/send_heal.rs); resolves to whether it did. Supplied by the send
/// path, which owns the AppState a restart needs.
pub type SessionChatSendHealer = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync,
>;

// ---------------------------------------------------------------------------
// Send probe
// ---------------------------------------------------------------------------

/*
CDXC:AgentScreenDetection 2026-08-19:
Sampled BEFORE the send is enqueued: everything appended to the transcript from
here on is a candidate for "the message arrived". Sampling afterwards would race
the agent's own write.
*/
pub struct SessionChatSendProbe {
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) zmx_name: String,
    pub(super) agent: Option<String>,
    pub(super) transcript_agent: SessionChatTranscriptAgent,
    pub(super) agent_session_id: Option<String>,
    pub(super) agent_session_path: Option<String>,
    /// `None` when the agent has not created its transcript yet (Codex creates
    /// the rollout lazily); the watchdog re-resolves it every tick.
    pub(super) transcript_path: Option<PathBuf>,
    pub(super) transcript_offset: u64,
    /*
    CDXC:AgentScreenDetection 2026-08-20:
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
    pub(super) intercepted: Option<InterceptedInput>,
    /// Wall-clock send time, for the deadline's last-chance scan: only a
    /// transcript file written after this instant can testify about the send.
    pub(super) sent_at: std::time::SystemTime,
    pub(super) text: String,
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
        session_chat_option_agent(agent)?;
        if text.trim().is_empty() {
            return None;
        }
        // CDXC:SessionChat 2026-09-27 WHY: Claude answers a `/btw` side question in a panel and writes nothing to its transcript, not even a `local_command` row, so this watchdog always found the send "never recorded" and replaced the side question card with a false "Claude Code is no longer running" notice. The panel itself is the delivery proof, and session_chat_claude_panel.rs reads it.
        // Codex answers its `/btw` in a side conversation that its main transcript never records either.
        if matches!(
            session_chat_option_agent(agent),
            Some(
                crate::session_chat_options::SessionChatOptionAgent::Claude
                    | crate::session_chat_options::SessionChatOptionAgent::Codex
            )
        ) && InterceptedInput::detect(text)
            == Some(InterceptedInput::SlashCommand("/btw".to_string()))
        {
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
CDXC:AgentScreenDetection 2026-08-19:
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
    returned_prompt: SessionChatReturnedPromptTrigger,
    heal: SessionChatSendHealer,
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
                returned_prompt,
                heal,
                generation,
                my_generation,
            ))
        },
    );
}

/// Which step of a refused send failed; the send-failure log names it.
/// CDXC:AgentScreenDetection 2026-10-09 WHY:
/// Every refused write used to share one card, "This session's terminal did not respond while the message was being typed into it". On one Windows machine all five of those cards (2026-10-04..07) were a paste that never showed in the input box (or Codex keeping a submitted message), with the terminal answering normally the whole time, and a 10.16.0 user reading it on macOS found "nothing special" in the terminal. The send-failure log says what actually failed so the next report names the step; the card itself is the plain Fix it card since the 2026-10-09 send recovery (session_chat_queue_runtime/send_heal.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendWriteFailure {
    /// The terminal refused the bytes or the input handshake never completed.
    TerminalUnresponsive,
    /// The message was typed but never appeared in the agent's input box.
    PasteNotShown,
    /// The input box ended up holding the message twice.
    PasteDoubled,
    /// The agent kept the message in its input box after Enter.
    NotSubmitted,
    /// The agent stayed mid-turn without taking new input for as long as the send waits.
    AgentStalled,
}

impl SendWriteFailure {
    pub(super) fn detail(self) -> &'static str {
        match self {
            Self::TerminalUnresponsive => "This session's terminal did not respond while the message was being typed into it, so the message was never delivered to the agent. Open the terminal to see what it is showing.",
            Self::PasteNotShown => "The message was typed into the terminal but never appeared in the agent's input box, so it was not sent. Open the terminal to see what it is showing, then send it again.",
            Self::PasteDoubled => "The message showed up twice in the agent's input box, so Ghostex cleared it instead of sending it. Send it again.",
            Self::NotSubmitted => "The agent kept the message in its input box after Enter instead of sending it, so it was not sent. Open the terminal to see what it is showing.",
            Self::AgentStalled => "The agent stayed in the middle of a turn for five minutes without taking new input, so the message was not sent. Open the terminal to see what it is showing.",
        }
    }
}

/*
CDXC:AgentScreenDetection 2026-08-19:
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
    failure: SendWriteFailure,
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
                    None,
                    UndeliveredSendReason::WriteFailed(failure),
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

/// Retires the session's watchdog without a verdict. The returned-prompt
/// detector calls this once it has proven the message is deliberately back in
/// the composer, so the deadline cannot fire a "not delivered" notice for it.
pub fn cancel_session_chat_send_watchdog(project_id: &str, session_id: &str) {
    let watchdogs = SESSION_CHAT_WATCHDOGS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut map) = watchdogs.lock() else {
        return;
    };
    if let Some(previous) = map.remove(&session_chat_notice_key(project_id, session_id)) {
        previous.generation.fetch_add(1, Ordering::SeqCst);
        previous.task.abort();
    }
}
