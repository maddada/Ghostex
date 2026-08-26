// Per-project parking bundle for the Agents session-chat CEF surfaces and the
// companion state keyed by the same project-local shell session ids.

use crate::*;

/*
CDXC:GPUISessionChatSurfacePerProject 2026-08-26:
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
    pub(crate) surfaces: HashMap<TerminalSessionId, Entity<CefSurface>>,
    pub(crate) surface_hidden_since: HashMap<TerminalSessionId, Instant>,
    pub(crate) composer_ready_sessions: HashSet<TerminalSessionId>,
    pub(crate) composer_empty_reports: HashMap<TerminalSessionId, bool>,
    pub(crate) pending_composer_focus: Option<TerminalSessionId>,
    pub(crate) pending_composer_insert: HashMap<TerminalSessionId, String>,
}

impl ParkedAgentsChatRuntime {
    /*
    CDXC:GPUISessionChatSurfaceEviction 2026-08-24 (extended 2026-08-26):
    Parked pages hold the same RAM as live hidden ones, so they age out on the
    same clock rather than living until the app quits. Each parked surface
    carried its hidden stamp into the park, and the project it belongs to
    rebuilds an evicted page through the idempotent `ensure_agents_chat_surface`
    on the first reconcile after the user comes back.

    Only the guards whose state travels in this bundle are checkable here, and
    each one keeps the original contract that unknown means "do not evict".
    */
    pub(crate) fn expired_hidden_evictable_session_ids(&self) -> Vec<TerminalSessionId> {
        self.surface_hidden_since
            .iter()
            .filter(|(session_id, hidden_since)| {
                hidden_since.elapsed() >= GPUI_AGENTS_CHAT_SURFACE_HIDDEN_EVICT_AFTER
                    && self.surfaces.contains_key(session_id)
                    && self.surface_evictable(**session_id)
            })
            .map(|(session_id, _)| *session_id)
            .collect()
    }

    /// Whether a parked chat surface holds nothing that would be destroyed with
    /// its page. Mirrors `agents_chat_surface_evictable` for the state a parked
    /// bundle owns.
    fn surface_evictable(&self, session_id: TerminalSessionId) -> bool {
        // A composer with typed text or attached images is unsent user content
        // that lives only in the page. Eviction requires an explicit "empty"
        // report plus a registered composer bridge; a missing entry means the
        // state is unknown, never empty.
        if !self.composer_ready_sessions.contains(&session_id)
            || self.composer_empty_reports.get(&session_id) != Some(&true)
        {
            return false;
        }
        // One-shot composer messages terminate at a page that has to still be
        // there to receive them once the project is activated again.
        if self.pending_composer_focus == Some(session_id)
            || self.pending_composer_insert.contains_key(&session_id)
        {
            return false;
        }
        true
    }
}
