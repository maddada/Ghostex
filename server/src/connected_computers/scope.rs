//! What another computer's agents may ask of this gxserver, and what this computer's relay
//! forwards: the endpoints the advertised verbs (`ghostex sessions`, `agents types/list/create/send/
//! close/reload`, `read-session-chat`, `read-text`, `send-message`/`send-text`/`send-key`,
//! `interrupt-session-chat`, `answer-session-chat-prompt`, `coordinator start-thread --computer`,
//! `coordinator promote --computer`, `link add|remove --session <thread on that computer>`,
//! the session sleep/wake/close actions) and the report checker for threads on another computer
//! call, and nothing else.

/// CDXC:RemoteMachines 2026-11-03 WHY:
/// A token another computer holds once opened every `RemoteAllowed` endpoint (about 200: extensions, git pushes, project deletion, automations, secrets, settings), and the relay forwarded any path, so A could reach B's local-only controls and relay on to C. Both now go through this one list, taken from the CLI verbs' own calls (grep `call_gxserver_rpc` in ghostex_cli/agents, sessions, wait, session_chat_rpc, actions/dispatch.rs and coordinator/threads.rs, plus server/coordinator_remote_runtime.rs). A new verb that should work across computers adds its endpoints here; `scope_tests` keeps every entry a `RemoteAllowed` endpoint.
/// SEE-ALSO: server/src/server/route_http/dispatch.rs (the gate), relay.rs (the forward), peer_listener.rs (the way back), server/src/voice_agent/computers.rs (Voice mode on another computer uses these same endpoints).
pub(crate) const PEER_ALLOWED_ENDPOINTS: &[&str] = &[
    // Reads: the session list, projects, agent types, the presentation the report checker reads.
    "/api/health/server",
    "/api/listProjects",
    "/api/listRecentProjects",
    "/api/listSessions",
    // The rows a session selector names by an id (`ghostex_cli` resolves one session with it).
    "/api/findSessions",
    "/api/readPresentationSnapshot",
    "/api/readSidebarHud",
    // `ghostex agents send --main-coordinator --computer <name>` finds that computer's main
    // coordinator of a project here (coordinators/main_coordinator.rs); ids, titles and goals.
    "/api/listCoordinators",
    "/api/readSessionChat",
    "/api/readSessionChatQueue",
    "/api/readSessionText",
    "/api/readSessionTerminalTail",
    // Starting, stopping and reloading sessions.
    "/api/createSession",
    "/api/createAgentSession",
    "/api/createWorktreeSession",
    "/api/startSessionProvider",
    "/api/killSession",
    "/api/sleepSession",
    "/api/wakeSession",
    "/api/reloadSessionAgent",
    // Messages, answers and keys.
    "/api/sendSessionChatMessage",
    "/api/queueSessionChatPrompt",
    "/api/updateSessionChatQueuedPrompt",
    "/api/answerSessionChatPrompt",
    "/api/interruptSessionChat",
    "/api/sendSessionText",
    "/api/sendSessionMessage",
    "/api/sendSessionEnter",
    "/api/watchAgentMessageDelivery",
    "/api/checkAgentSessionWrite",
    // CDXC:Coordinators 2026-11-07 FIX: `ghostex coordinator promote <ref> --computer <name>`
    // (Discord: "why orchestrator cannot be promoted to a session in a remote machine from the
    // desktop app?"). The other computer records the coordinator, installs its own bundled
    // orchestrator skill and sends the playbook message: no more than `sendSessionChatMessage`
    // above already lets an agent do, plus the role.
    "/api/promoteCoordinator",
    // `ghostex agent-hooks status|install|uninstall --computer <name>`: Ghostex's own session
    // resume hooks on that computer, which a headless one has no other way to get (issue #431,
    // CDXC:AgentHooks 2026-11-07 FIX in agent_hooks/peer_start.rs).
    "/api/readAgentHookStatus",
    "/api/installAgentHooks",
    "/api/uninstallAgentHooks",
    // CDXC:SessionLinks 2026-11-08 FIX: `ghostex link add|remove --session <thread on that
    // computer>` (issue #497: a coordinator links every thread to its PR). Only the relay on the
    // coordinator's own computer knows whose thread a session is, so it limits these two to the
    // coordinator's own threads (thread_scope.rs); the other link verbs stay off.
    "/api/addSessionLink",
    "/api/removeSessionLink",
    // `ghostex chief send --computer <name>` reads that computer's Chief here (`status`); the
    // handler refuses a peer anything but `status` and `guide` (chief/endpoint.rs).
    "/api/chief",
    // A GBot's chat there reading its brief and keeping its memory here (gbots/computer/home.rs).
    // Only gxserver itself sends it: the relay never forwards it (relay.rs `relay_forwards`).
    "/api/gbotsRemote",
    // Settings sync: one exchange trades both computers' synced settings (server/src/settings_sync/).
    "/api/settingsSyncExchange",
];

pub(crate) fn is_peer_allowed(path: &str) -> bool {
    PEER_ALLOWED_ENDPOINTS.contains(&path)
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use crate::protocol::{endpoint_for, ApiPermission, Transport};

    #[test]
    fn every_endpoint_another_computer_may_use_is_a_remote_allowed_http_endpoint() {
        for path in PEER_ALLOWED_ENDPOINTS {
            let endpoint = endpoint_for(path).unwrap_or_else(|| panic!("{path} is no endpoint"));
            assert_eq!(endpoint.transport, Transport::Http, "{path}");
            assert_eq!(endpoint.permission, ApiPermission::RemoteAllowed, "{path}");
            assert!(endpoint.requires_auth, "{path}");
        }
    }
}
