use super::*;

// ---------------------------------------------------------------------------
// Per-session send queue (upstream chat spec §7.6)
// ---------------------------------------------------------------------------

/*
CDXC:AgentScreenDetection 2026-08-19:
Why a send did not complete. The message a caller shows the user is the same as
before; what is new is that the caller can tell "the terminal refused this
message" (the agent CLI in the pane never answered the Ctrl+G handshake, or zmx
would not take the bytes — the crashed-agent case this feature exists to
explain) apart from "this send was never attempted" (superseded by a newer send,
cancelled, or the queue was gone). Only the former is evidence about the
terminal, so only the former may raise a notice.
*/
pub(super) const SESSION_CHAT_SEND_CANCELLED: &str = "The session chat send was cancelled.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatSendFailure {
    /// Superseded, cancelled, or never dequeued: the terminal was never asked.
    NotAttempted,
    /// The Ctrl+G draft-preservation handshake never completed.
    PreserveTerminalDraft,
    /// A `zmx send` burst was refused.
    Write,
    /*
    CDXC:SessionChat 2026-08-26:
    The agent CLI never showed an input box within the wait. Distinct from
    `Write` because nothing was written: there is no half-typed composer to
    explain and nothing for the delivery watchdog to verify, and the caller maps
    it to its own `composerNotReady` code so the UI can say what is on the
    screen instead of "the terminal refused the input".
    */
    ComposerNotReady,
    ComposerNotCleared,
    /// The send's guard found a prompt waiting on the user (a dialog, a question, a card its hooks
    /// raised) and typed nothing into it (session_chat_send/guarded_write.rs). Callers map it to
    /// `promptWaiting`: the message waits for the answer, which is no missing input box.
    PromptWaiting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatSendError {
    pub failure: SessionChatSendFailure,
    pub message: String,
}

impl SessionChatSendError {
    pub(crate) fn new(failure: SessionChatSendFailure, message: String) -> Self {
        Self { failure, message }
    }

    pub(super) fn not_attempted(message: String) -> Self {
        Self::new(SessionChatSendFailure::NotAttempted, message)
    }

    /// True when the session's own terminal is what refused the message —
    /// which is what makes a watchdog escalation (one more capture, to explain
    /// the refusal) worth spending. A composer that never appeared already
    /// carries its own explanation, so it is not one of these.
    pub fn terminal_refused(&self) -> bool {
        matches!(
            self.failure,
            SessionChatSendFailure::PreserveTerminalDraft | SessionChatSendFailure::Write
        )
    }

    /// True when the send stopped because the agent CLI had no input box.
    pub fn composer_not_ready(&self) -> bool {
        self.failure == SessionChatSendFailure::ComposerNotReady
    }

    /// True when the send's guard stopped it because a prompt waits on the user.
    pub fn prompt_waiting(&self) -> bool {
        self.failure == SessionChatSendFailure::PromptWaiting
    }

    /// True when the send was cancelled before its Enter was written: the
    /// interrupt endpoint bumped the queue generation under it, so nothing
    /// reached the agent and the caller still owns the text.
    pub fn cancelled(&self) -> bool {
        self.failure == SessionChatSendFailure::NotAttempted
            && self.message == SESSION_CHAT_SEND_CANCELLED
    }
}

pub(crate) type SessionChatStepObserver = Arc<dyn Fn(&SessionChatSendStep) + Send + Sync>;

pub(super) struct SessionChatSendJob {
    pub(super) on_step: Option<SessionChatStepObserver>,
    pub(super) completion: Option<oneshot::Sender<Result<(), SessionChatSendError>>>,
    /// Set only by `capture_session_chat_terminal_draft`: the standalone
    /// draft-handoff endpoint needs the draft its `PreserveTerminalDraft` step
    /// captured, not merely whether the step succeeded.
    pub(super) captured_draft: Option<oneshot::Sender<CapturedTerminalDraft>>,
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) source: String,
    pub(super) zmx_name: String,
    pub(super) generation: u64,
    pub(super) steps: Vec<SessionChatSendStep>,
}

struct SessionChatSendQueue {
    tx: mpsc::UnboundedSender<SessionChatSendJob>,
    generation: Arc<AtomicU64>,
}

static SESSION_CHAT_SEND_QUEUES: OnceLock<Mutex<HashMap<String, SessionChatSendQueue>>> =
    OnceLock::new();

fn queue_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

/*
Invariant preserved from the upstream chat spec: each sequence owns the input line from its
clear until its Enter fires. One worker task per session drains jobs
serially; a cancelled generation skips queued jobs at dequeue AND aborts the
remaining steps of an in-flight job before its next write/sleep. A failed
zmx write aborts the rest of its sequence so a dangling Enter can never
follow a body that was not delivered. Must be called from within the tokio
runtime (HTTP handlers are).

CDXC:SessionChat 2026-08-24:
"Each sequence owns the input line" only holds if EVERY server-side writer to
that pty goes through here. Until this date several did not — the first-prompt
auto-title job, the manual "generate name" job (which writes a Ctrl+U draft
kill!), delayed sends, the raw `/api/sendSessionText` + `/api/sendSessionEnter`
endpoints, and the draft-handoff BEL — and they interleaved with in-flight send
sequences (measured: an auto-title write landed 70ms after a corrupted send,
because the auto-title job is triggered by the FIRST user prompt, i.e. exactly
while that prompt is still being delivered). They all enqueue now, and a writer
whose bytes must not be split apart (text → settle → Enter) enqueues them as ONE
job, because separate jobs may be separated by somebody else's job.
*/
pub fn enqueue_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
) {
    let _ = queue_session_chat_send(
        project_id, session_id, zmx_name, source, steps, None, None, None,
    );
}

/// The same fire-and-forget enqueue for a server-side writer that holds a
/// repository session row rather than a resolved zmx name. The name is resolved
/// exactly as the zmx endpoints resolve it, so a session whose provider is not
/// a zmx pty is rejected instead of written to blindly.
pub fn enqueue_session_write_sequence(
    session: &Value,
    project_id: &str,
    session_id: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
) -> std::result::Result<(), DomainStateError> {
    let zmx_name = crate::zmx::provider_zmx_session_name(session)?;
    enqueue_session_chat_send(project_id, session_id, &zmx_name, source, steps);
    Ok(())
}

/// `enqueue_session_write_sequence` for a writer that needs the outcome: the
/// returned receiver resolves once every step ran, or with the step that
/// stopped the sequence.
pub(crate) fn enqueue_session_write_sequence_with_completion(
    session: &Value,
    project_id: &str,
    session_id: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
    on_step: Option<SessionChatStepObserver>,
) -> std::result::Result<oneshot::Receiver<Result<(), SessionChatSendError>>, DomainStateError> {
    let zmx_name = crate::zmx::provider_zmx_session_name(session)?;
    let (completion_tx, completion_rx) = oneshot::channel();
    queue_session_chat_send(
        project_id,
        session_id,
        &zmx_name,
        source,
        steps,
        Some(completion_tx),
        None,
        on_step,
    )
    .map_err(|message| DomainStateError {
        code: "internalError",
        message,
    })?;
    Ok(completion_rx)
}

/// Enqueues one sequence on the same per-session worker as fire-and-forget
/// sends, but resolves only after every preservation/write step has completed.
/// Chat message HTTP calls use this so the composer is cleared only after the
/// terminal draft is safe and the new prompt was actually submitted.
pub async fn execute_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
) -> Result<(), SessionChatSendError> {
    let (completion_tx, completion_rx) = oneshot::channel();
    queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        source,
        steps,
        Some(completion_tx),
        None,
        None,
    )
    .map_err(SessionChatSendError::not_attempted)?;
    completion_rx.await.map_err(|_| {
        SessionChatSendError::not_attempted(
            "The session chat send worker stopped before completing the message.".to_string(),
        )
    })?
}

pub(super) fn queue_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    steps: Vec<SessionChatSendStep>,
    completion: Option<oneshot::Sender<Result<(), SessionChatSendError>>>,
    captured_draft: Option<oneshot::Sender<CapturedTerminalDraft>>,
    on_step: Option<SessionChatStepObserver>,
) -> Result<(), String> {
    if steps.is_empty() {
        return Ok(());
    }
    let queues = SESSION_CHAT_SEND_QUEUES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = queues
        .lock()
        .map_err(|_| "The session chat send queue is unavailable.".to_string())?;
    let queue = map
        .entry(queue_key(project_id, session_id))
        .or_insert_with(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            let generation = Arc::new(AtomicU64::new(0));
            tokio::spawn(run_session_chat_send_worker(rx, generation.clone()));
            SessionChatSendQueue { tx, generation }
        });
    let job = SessionChatSendJob {
        on_step,
        completion,
        captured_draft,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        source: source.to_string(),
        zmx_name: zmx_name.to_string(),
        generation: queue.generation.load(Ordering::SeqCst),
        steps,
    };
    queue
        .tx
        .send(job)
        .map_err(|_| "The session chat send worker is unavailable.".to_string())
}

/// Cancels every queued (and the remaining steps of any in-flight) send for a
/// session by bumping its generation. Later enqueues use the new generation.
pub fn cancel_session_chat_sends(project_id: &str, session_id: &str) {
    let Some(queues) = SESSION_CHAT_SEND_QUEUES.get() else {
        return;
    };
    let Ok(map) = queues.lock() else {
        return;
    };
    if let Some(queue) = map.get(&queue_key(project_id, session_id)) {
        queue.generation.fetch_add(1, Ordering::SeqCst);
    }
}
