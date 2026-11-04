//! What a host passes to [`crate::GxClient::start`], and the client's timing rules.

use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ghostex_gx_core::MachineId;

/// Delay before reconnect attempt `n` (the last entry repeats).
///
/// CDXC:Sidebar 2026-10-08 WHY: the ladder used to climb to 16 s, so a sidebar that started while
/// the daemon was down said "Unable to load sessions." long after the daemon was back. It now
/// stops at the user's 2 s retry interval, one connect attempt at a time, for as long as the client
/// runs. SEE-ALSO: packages/gx-core/src/sidebar_view/daemon_wait.rs (`DAEMON_RETRY_INTERVAL_MS`).
pub const RECONNECT_LADDER_MS: [u64; 3] = [250, 1000, ghostex_gx_core::DAEMON_RETRY_INTERVAL_MS];

/// The ladder restarts from its first step only when the stream had been acknowledged for at
/// least this long before it dropped. A stream that flaps faster keeps escalating.
pub const HEALTHY_STREAM_DURATION: Duration = Duration::from_secs(30);

/// A subscribe the daemon cannot serve (state database unavailable) is answered by silence, so
/// every subscribe is paired with this deadline; missing it drops the socket and reconnects.
///
/// CDXC:RemoteMachines 2026-10-31 WHY: the acknowledgement is the full snapshot frame, a few
/// hundred kilobytes to a few megabytes, and over a slow link to a remote machine (GitHub #361) it
/// could not arrive in 10 s: the socket was dropped mid-snapshot, the sidebar went grey while the
/// terminal kept flowing, and the reconnect asked for the whole snapshot again. Measured through
/// a 1 Mbps, 300 ms, 2% loss link: a 1.5 MB snapshot was never acknowledged in 90 s. The deadline
/// now waits while that frame is still arriving and only silence for this long ends the socket
/// (`subscribe_ack_overdue`); a remote machine's frame can wait behind other traffic on its
/// tunnel, so its silence may last [`REMOTE_SUBSCRIBE_ACK_TIMEOUT`] (`subscribe_ack_timeout`).
pub const SUBSCRIBE_ACK_TIMEOUT: Duration = Duration::from_secs(10);
/// [`SUBSCRIBE_ACK_TIMEOUT`] for a remote machine. Through a 1 s, 8% loss link whose chat reads
/// filled it, the snapshot's first bytes came more than 10 s after the subscribe.
pub const REMOTE_SUBSCRIBE_ACK_TIMEOUT: Duration = Duration::from_secs(30);

/// Lower bound between two resubscribes the client forces itself after a delta failed to parse.
pub const FORCED_RESUBSCRIBE_INTERVAL: Duration = Duration::from_secs(10);

/// How long one socket read may block. Bounds how late the thread notices a shutdown or a
/// resubscribe request, so dropping the client never waits on the daemon.
pub(crate) const SOCKET_READ_TIMEOUT: Duration = Duration::from_millis(200);
pub(crate) const SOCKET_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// A remote machine's handshake crosses its SSH tunnel, where it waits behind whatever the link is
/// still carrying (a dropped socket's snapshot keeps draining through SSH). Measured through a
/// 1 Mbps, 300 ms link, 5 s timed out on most reconnects (GitHub #361). The loopback daemon keeps 5 s.
pub(crate) const REMOTE_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// The `listProjects` read runs on the socket thread (see `worker.rs` for why), so it is short.
pub(crate) const DOMAIN_PROJECTS_READ_TIMEOUT: Duration = Duration::from_secs(3);
/// A snapshot of a large workspace is a few megabytes; the default tungstenite frame cap of
/// 16 MiB would turn a larger one into an endless reconnect loop.
pub(crate) const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

/// How long the handshake may wait for the daemon on `machine`.
pub(crate) fn handshake_timeout(machine: &MachineId) -> Duration {
    match machine {
        MachineId::Local => HANDSHAKE_TIMEOUT,
        _ => REMOTE_HANDSHAKE_TIMEOUT,
    }
}

/// How long a subscribe on `machine` may go without an answer.
pub(crate) fn subscribe_ack_timeout(machine: &MachineId) -> Duration {
    match machine {
        MachineId::Local => SUBSCRIBE_ACK_TIMEOUT,
        _ => REMOTE_SUBSCRIBE_ACK_TIMEOUT,
    }
}

/// Whether a subscribe whose deadline is `deadline` has gone unanswered. The acknowledgement is
/// one large frame, so the deadline holds while a frame is still arriving: bytes came in after the
/// last whole message, the latest of them less than `silence` ago. Small whole frames that are not
/// the acknowledgement do not hold it.
pub(crate) fn subscribe_ack_overdue(
    now: Instant,
    deadline: Instant,
    silence: Duration,
    last_byte_at: Option<Instant>,
    last_message_at: Option<Instant>,
) -> bool {
    if now < deadline {
        return false;
    }
    let frame_arriving = match (last_byte_at, last_message_at) {
        (Some(byte), Some(message)) => byte > message,
        (Some(_), None) => true,
        (None, _) => false,
    };
    !(frame_arriving
        && last_byte_at.is_some_and(|byte| now.saturating_duration_since(byte) < silence))
}

/// Where the daemon is and how the store is identified to it.
#[derive(Clone)]
pub struct GxClientConfig {
    /// The machine the events are reported for. The local daemon is [`MachineId::Local`].
    pub machine: MachineId,
    /// `http://<loopback host>:<port>`, without a path. Any other scheme is refused.
    pub base_url: String,
    /// The daemon's bearer token. Sent in the `Authorization` header only, never in a URL.
    pub auth_token: String,
    /// Echoed back on the snapshot frame; the daemon does not route by it.
    pub client_id: String,
    /// The revision the host's store holds for `machine`, `0` while it holds nothing. The client
    /// quotes it as `lastRevision` on every subscribe; the host writes it after applying frames.
    pub held_revision: Arc<AtomicI64>,
    /// Parse and forward the four `sessionChat*` frames. A full-stream socket carries the chat
    /// frames of every session any client subscribed to; the core has no state for them before
    /// the chat milestone, so a host leaves this off and they are dropped unparsed like
    /// `apiRequestHandled`. Tools turn it on to check the chat wire types against live traffic.
    pub forward_chat_frames: bool,
    /// Registers this socket as the daemon's renderer-command target (`rendererCommands: true` on
    /// every subscribe) and hands each `rendererCommand` frame to the host as
    /// [`crate::ClientOutput::RendererCommand`], which the host must answer with
    /// [`crate::GxClient::answer_renderer_command`]. Only the desktop's local store turns it on: the
    /// daemon sends every command to the FIRST open socket that registered, so a second registrant
    /// in the same app would split the CLI's commands between two answerers.
    pub renderer_commands: bool,
}

/// Written by hand so the bearer token can never reach a log through `{:?}`.
impl std::fmt::Debug for GxClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GxClientConfig")
            .field("machine", &self.machine)
            .field("base_url", &self.base_url)
            .field("auth_token", &"[redacted]")
            .field("client_id", &self.client_id)
            .field("held_revision", &self.held_revision)
            .field("forward_chat_frames", &self.forward_chat_frames)
            .field("renderer_commands", &self.renderer_commands)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: Duration = SUBSCRIBE_ACK_TIMEOUT;

    #[test]
    fn a_snapshot_still_arriving_holds_the_acknowledgement_deadline() {
        let subscribed = Instant::now();
        let deadline = subscribed + LOCAL;
        let later = |seconds: u64| subscribed + Duration::from_secs(seconds);
        // Before the deadline nothing is overdue.
        assert!(!subscribe_ack_overdue(
            later(5),
            deadline,
            LOCAL,
            None,
            None
        ));
        // Silence past the deadline is the daemon not serving the subscribe.
        assert!(subscribe_ack_overdue(
            later(10),
            deadline,
            LOCAL,
            None,
            None
        ));
        // A big frame has been trickling in since the subscribe, its last bytes 1 s ago.
        assert!(!subscribe_ack_overdue(
            later(40),
            deadline,
            LOCAL,
            Some(later(39)),
            None
        ));
        // It stalled: no byte for the whole timeout.
        assert!(subscribe_ack_overdue(
            later(40),
            deadline,
            LOCAL,
            Some(later(29)),
            None
        ));
    }

    #[test]
    fn small_whole_frames_do_not_hold_the_deadline() {
        let subscribed = Instant::now();
        let deadline = subscribed + LOCAL;
        let later = |seconds: u64| subscribed + Duration::from_secs(seconds);
        // An older daemon's request frames arrive whole: the last byte belongs to a message that
        // was already read.
        assert!(subscribe_ack_overdue(
            later(12),
            deadline,
            LOCAL,
            Some(later(11)),
            Some(later(11))
        ));
        // Bytes after the last message are the next frame on its way.
        assert!(!subscribe_ack_overdue(
            later(12),
            deadline,
            LOCAL,
            Some(later(12)),
            Some(later(11))
        ));
    }

    #[test]
    fn a_remote_machine_waits_longer_through_a_busy_tunnel() {
        let remote = MachineId::Remote("remote-a".into());
        assert_eq!(
            subscribe_ack_timeout(&MachineId::Local),
            SUBSCRIBE_ACK_TIMEOUT
        );
        assert_eq!(subscribe_ack_timeout(&remote), REMOTE_SUBSCRIBE_ACK_TIMEOUT);
        let silence = subscribe_ack_timeout(&remote);
        let subscribed = Instant::now();
        let deadline = subscribed + silence;
        let later = |seconds: u64| subscribed + Duration::from_secs(seconds);
        // The snapshot's first bytes are still queued behind chat reads 12 s in.
        assert!(!subscribe_ack_overdue(
            later(12),
            deadline,
            silence,
            None,
            None
        ));
        // A remote daemon that never answers is still given up on.
        assert!(subscribe_ack_overdue(
            later(30),
            deadline,
            silence,
            None,
            None
        ));
    }

    #[test]
    fn only_a_remote_handshake_gets_the_slow_link_patience() {
        assert_eq!(handshake_timeout(&MachineId::Local), HANDSHAKE_TIMEOUT);
        assert_eq!(
            handshake_timeout(&MachineId::Remote("remote-a".into())),
            REMOTE_HANDSHAKE_TIMEOUT
        );
    }
}
