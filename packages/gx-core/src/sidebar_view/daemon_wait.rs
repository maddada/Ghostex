//! What the sidebar says while this computer's Ghostex service is not answering, and how often the
//! clients try again.

use super::inputs::UnavailableState;

/// How often a client tries Ghostex's service again while it is not answering: the store's socket
/// (`packages/gx-client`), the web page's socket and bootstrap, and the chat's own retries
/// (`packages/gx-chat-core`, `packages/gx-chat-client`).
///
/// CDXC:Sidebar 2026-10-08 DECISION:
/// User: "please make these messages easier to understand and also please make it keep retrying every 2 seconds itself until gx server is there instead of just requiring the user to click". While the service is unreachable every client retries on its own every 2 seconds, with no limit while the window is open, one attempt in flight at a time, and recovers silently when it answers. The empty sidebar says "Loading sessions…" instead of "Unable to load sessions.", and keeps a Try now button for impatient users. The user rejected "Connecting to Ghostex" ("seems strange that it's connecting to itself"): the copy is about loading sessions, never about connecting to Ghostex, and after about 30 s it reads "Sessions are taking longer than usual to load. If this keeps happening, restart Ghostex."
/// SEE-ALSO: packages/gx-client/src/config.rs (`RECONNECT_LADDER_MS`), packages/gx-chat-core/src/session/constants.rs (`UNREACHABLE_RETRY_DELAY_MS`), packages/gx-chat-client/src/wire.rs (`RECONNECT_DELAYS_MS`), apps/gpui-web/src/app/gx_store/host.rs.
pub const DAEMON_RETRY_INTERVAL_MS: u64 = 2_000;

/// The sidebar's headline and detail while this computer's sessions have not loaded. The native
/// views that need a project (Files, Kanban, Automate, extension views) say the same words in
/// place of "unavailable" until the sessions arrive.
pub const SESSIONS_LOADING_TITLE: &str = "Loading sessions\u{2026}";
pub const SESSIONS_LOADING_DETAIL: &str = "They\u{2019}ll appear in a moment.";

/// How long a first connection may take before the skeleton gives way to "Loading sessions…".
/// A cold start on Windows can take this long without anything being wrong.
const CONNECTING_AFTER_MS: u64 = 10_000;
/// How long the wait runs before the sidebar suggests restarting Ghostex.
const STILL_WAITING_AFTER_MS: u64 = 30_000;

/// Which picture the empty sidebar draws while this computer has not loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DaemonWait {
    /// The first moments of a start: the skeleton, no copy.
    Skeleton,
    /// "Loading sessions…", with Try now.
    Connecting,
    /// The wait has run long: "taking longer than usual", suggest a restart, with Try now.
    StillWaiting,
}

impl DaemonWait {
    /// The phase for how long the service has been away. A sidebar that had loaded once skips the
    /// skeleton: the user already saw their sessions, so an empty pulse would read as data loss.
    pub(crate) fn at(unavailable: UnavailableState, now_ms: u64) -> Self {
        let waited = unavailable
            .since_ms
            .map(|since| now_ms.saturating_sub(since))
            .unwrap_or(0);
        if waited >= STILL_WAITING_AFTER_MS {
            Self::StillWaiting
        } else if unavailable.observed_available || waited >= CONNECTING_AFTER_MS {
            Self::Connecting
        } else {
            Self::Skeleton
        }
    }

    /// The headline and the line under it.
    pub(crate) fn copy(self) -> (&'static str, &'static str) {
        match self {
            Self::Skeleton => ("", ""),
            Self::Connecting => (SESSIONS_LOADING_TITLE, SESSIONS_LOADING_DETAIL),
            Self::StillWaiting => (
                "Sessions are taking longer than usual to load.",
                "If this keeps happening, restart Ghostex.",
            ),
        }
    }

    /// The next host time the phase moves on its own, so the list is rebuilt then.
    pub(crate) fn next_change_ms(unavailable: UnavailableState, now_ms: u64) -> Option<u64> {
        let since = unavailable.since_ms?;
        [CONNECTING_AFTER_MS, STILL_WAITING_AFTER_MS]
            .into_iter()
            .map(|after| since.saturating_add(after))
            .find(|at| *at > now_ms)
    }
}

/// The empty state's manual retry button.
pub(crate) const TRY_NOW_LABEL: &str = "Try now";
