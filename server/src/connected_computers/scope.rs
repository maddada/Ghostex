//! What another computer's agents may ask of this gxserver, and what this computer's relay
//! forwards: the endpoints the advertised verbs (`ghostex sessions`, `agents types/list/create/send/
//! close/reload`, `read-session-chat`, `read-text`, `send-message`/`send-text`/`send-key`,
//! `interrupt-session-chat`, `answer-session-chat-prompt`, `coordinator start-thread --computer`,
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
    "/api/readPresentationSnapshot",
    "/api/readSidebarHud",
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
