//! What the client thread hands the host.

use std::sync::atomic::{AtomicU64, Ordering};

use ghostex_gx_core::Event;
use ghostex_gx_protocol::RendererCommand;

/// One item of the client's channel.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientOutput {
    /// Feed it to `Core::handle_batch` in the order received.
    Event(Event),
    /// Something the host should log. Never carries frame content.
    Diagnostic(ClientDiagnostic),
    /// A command the daemon dispatched to this socket (only when the config registered it as the
    /// renderer-command target). The host performs it and answers with
    /// [`crate::GxClient::answer_renderer_command`] before the command's `timeout_ms`.
    RendererCommand(RendererCommand),
}

/// A problem worth a log line. Payloads never appear here: a frame is described by its `type`
/// and by the parser's message with every quoted value removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientDiagnostic {
    /// A frame did not fit its type. When it was a `presentationDelta` the store has missed a
    /// change, so the client resubscribes without `lastRevision` (rate limited); the snapshot
    /// path then skips and reports the bad row instead of failing the whole frame.
    FrameParseFailed {
        event_type: Option<String>,
        error: String,
        resubscribe_scheduled: bool,
    },
    /// `/api/listProjects` could not be read after a subscribe. The store keeps the domain rows
    /// it has; chat project detection then rests on the project paths alone.
    DomainProjectsReadFailed { error: String },
    /// The daemon did not answer a subscribe in time; the socket is dropped and reopened.
    SubscribeNotAcknowledged,
    /// The daemon speaks another protocol version than this client.
    ProtocolMismatch { received: u64 },
    /// The socket thread ended without being asked to (a panic, with its message stripped of
    /// quoted values). It is the last output of this client: nothing reconnects any more, so the
    /// host must start a new client.
    ThreadStopped { reason: String },
}

/// Counters the socket thread keeps, readable from any thread.
#[derive(Debug, Default)]
pub struct ClientStats {
    pub(crate) connects: AtomicU64,
    pub(crate) subscribes: AtomicU64,
    pub(crate) frames_received: AtomicU64,
    pub(crate) bytes_received: AtomicU64,
    pub(crate) api_request_handled_dropped: AtomicU64,
    pub(crate) chat_frames_dropped: AtomicU64,
    pub(crate) frames_forwarded: AtomicU64,
    pub(crate) parse_failures: AtomicU64,
    pub(crate) domain_project_reads: AtomicU64,
    pub(crate) renderer_commands: AtomicU64,
    pub(crate) renderer_answers: AtomicU64,
}

/// A copy of [`ClientStats`] at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClientStatsSnapshot {
    /// Sockets that completed the handshake.
    pub connects: u64,
    /// `subscribePresentation` messages sent.
    pub subscribes: u64,
    pub frames_received: u64,
    pub bytes_received: u64,
    /// `apiRequestHandled` frames dropped without parsing.
    pub api_request_handled_dropped: u64,
    /// `sessionChat*` frames dropped without parsing.
    pub chat_frames_dropped: u64,
    /// Frames parsed and handed to the host.
    pub frames_forwarded: u64,
    pub parse_failures: u64,
    /// Successful `/api/listProjects` reads.
    pub domain_project_reads: u64,
    /// `rendererCommand` frames handed to the host.
    pub renderer_commands: u64,
    /// `rendererCommandResult` messages written back to the daemon.
    pub renderer_answers: u64,
}

impl ClientStats {
    pub fn snapshot(&self) -> ClientStatsSnapshot {
        let read = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        ClientStatsSnapshot {
            connects: read(&self.connects),
            subscribes: read(&self.subscribes),
            frames_received: read(&self.frames_received),
            bytes_received: read(&self.bytes_received),
            api_request_handled_dropped: read(&self.api_request_handled_dropped),
            chat_frames_dropped: read(&self.chat_frames_dropped),
            frames_forwarded: read(&self.frames_forwarded),
            parse_failures: read(&self.parse_failures),
            domain_project_reads: read(&self.domain_project_reads),
            renderer_commands: read(&self.renderer_commands),
            renderer_answers: read(&self.renderer_answers),
        }
    }
}

pub(crate) fn bump(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Replaces every double-quoted run in a parser message with `"…"`. serde_json quotes the
/// offending value (`invalid type: string "..."`), and that value can be a title or a path; field
/// names are in backticks and stay. Hosts use it for any serde message they log, such as the
/// `first_error` of `Effect::ReportSkippedRows`.
pub fn redact_quoted_values(message: &str) -> String {
    let mut redacted = String::with_capacity(message.len());
    let mut characters = message.chars();
    while let Some(character) = characters.next() {
        if character != '"' {
            redacted.push(character);
            continue;
        }
        redacted.push_str("\"…\"");
        let mut escaped = false;
        for quoted in characters.by_ref() {
            match quoted {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => break,
                _ => {}
            }
        }
    }
    redacted
}
