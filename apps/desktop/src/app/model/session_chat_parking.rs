// Per-project parking bundle for the Agents session-chat CEF surfaces and the
// companion state keyed by the same project-local shell session ids.

use crate::*;

/*
CDXC:SessionChat 2026-08-26:
Inactive project workspaces keep their live chat pages beside their parked shell
models, exactly like `ParkedAgentsTerminalRuntime` keeps their terminal owners.
Dropping the `Entity<CefSurface>` closes the Chromium browser, so a project
switch that destroyed them forced a visible kill + reload of every chat pane on
the way back.

Shell session ids are per-`WorkspaceModel` counters and therefore collide across
projects, so every companion map keyed by those ids travels in the same bundle:
leaving one behind would let the incoming project's ids read the outgoing
project's composer readiness, emptiness reports, or one-shot composer messages.
None of this is serialized; it returns only to the exact project that parked it.
*/
#[derive(Default)]
pub(crate) struct ParkedAgentsChatRuntime {
    pub(crate) page_states: HashMap<TerminalSessionId, SessionChatPageState>,
    /// Pending sends and handoffs remain protected until the owning project restores their runtime state.
    pub(crate) protected_sessions: HashSet<TerminalSessionId>,
    pub(crate) surfaces: HashMap<TerminalSessionId, Entity<CefSurface>>,
    pub(crate) surface_hidden_since: HashMap<TerminalSessionId, Instant>,
    pub(crate) composer_ready_sessions: HashSet<TerminalSessionId>,
    pub(crate) composer_empty_reports: HashMap<TerminalSessionId, bool>,
    pub(crate) pending_composer_focus: Option<TerminalSessionId>,
    pub(crate) pending_composer_insert: HashMap<TerminalSessionId, String>,
}

impl ParkedAgentsChatRuntime {
    pub(crate) fn surface_evictable(
        &self,
        session_id: TerminalSessionId,
        require_empty: bool,
    ) -> bool {
        self.composer_ready_sessions.contains(&session_id)
            && (!require_empty || self.composer_empty_reports.get(&session_id) == Some(&true))
            && !self.protected_sessions.contains(&session_id)
            && self.pending_composer_focus != Some(session_id)
            && !self.pending_composer_insert.contains_key(&session_id)
    }
}

/// CDXC:SessionChat 2026-09-05 WHY:
/// Numeric shell session IDs collide across projects, and a browser can finish posting after its replacement exists.
/// The generation travels with the exact page through parking; a hidden-page probe is cancelled whenever that page is shown again.
pub(crate) struct SessionChatPageState {
    pub(crate) generation: u64,
    pub(crate) pending_probe: Option<(u64, Option<futures::channel::oneshot::Sender<bool>>)>,
}

impl SessionChatPageState {
    pub(crate) fn next_identity() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn new() -> Self {
        Self {
            generation: Self::next_identity(),
            pending_probe: None,
        }
    }
}
