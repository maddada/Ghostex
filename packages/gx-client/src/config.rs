//! What a host passes to [`crate::GxClient::start`], and the client's timing rules.

use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::Duration;

use ghostex_gx_core::MachineId;

/// Delay before reconnect attempt `n` (the last entry repeats). The same ladder the TypeScript
/// client uses, so a daemon restart is met by one client behaviour, not two.
pub const RECONNECT_LADDER_MS: [u64; 6] = [250, 1000, 2000, 4000, 8000, 16000];

/// The ladder restarts from its first step only when the stream had been acknowledged for at
/// least this long before it dropped. A stream that flaps faster keeps escalating.
pub const HEALTHY_STREAM_DURATION: Duration = Duration::from_secs(30);

/// A subscribe the daemon cannot serve (state database unavailable) is answered by silence, so
/// every subscribe is paired with this deadline; missing it drops the socket and reconnects.
pub const SUBSCRIBE_ACK_TIMEOUT: Duration = Duration::from_secs(10);

/// Lower bound between two resubscribes the client forces itself after a delta failed to parse.
pub const FORCED_RESUBSCRIBE_INTERVAL: Duration = Duration::from_secs(10);

/// How long one socket read may block. Bounds how late the thread notices a shutdown or a
/// resubscribe request, so dropping the client never waits on the daemon.
pub(crate) const SOCKET_READ_TIMEOUT: Duration = Duration::from_millis(200);
pub(crate) const SOCKET_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// The `listProjects` read runs on the socket thread (see `worker.rs` for why), so it is short.
pub(crate) const DOMAIN_PROJECTS_READ_TIMEOUT: Duration = Duration::from_secs(3);
/// A snapshot of a large workspace is a few megabytes; the default tungstenite frame cap of
/// 16 MiB would turn a larger one into an endless reconnect loop.
pub(crate) const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;

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
