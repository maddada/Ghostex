//! Waiting for a box session's agent to accept input before Ghostex types into it.

use std::time::{Duration, Instant};

use super::session::session_agentbox;
use crate::domain::DomainRepository;
use crate::paths::GxserverPaths;
use crate::session_chat_composer::{
    wait_for_session_chat_composer_by_ids, SessionChatComposerWait, SessionChatComposerWaitPolicy,
};
use crate::storage::open_gxserver_database;

/// How long a box may take to come up, including a first-time Claude sign-in.
const BOX_CREATE_WAIT: Duration = Duration::from_secs(15 * 60);
const BOX_CREATE_POLL: Duration = Duration::from_secs(3);
/// How long the agent may take to paint its input box once the box exists.
const BOX_INPUT_WAIT_MS: u64 = 60_000;

/// Whether this session runs in a box, read now from the registry.
pub(crate) fn is_agentbox_session_by_ids(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> bool {
    open_gxserver_database(paths)
        .ok()
        .and_then(|db| {
            DomainRepository::new(&db, server_id)
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .is_some_and(|session| session_agentbox(&session).is_some())
}

/// Waits until the box exists and its agent shows its input box. `false` when the box never came
/// up, the session stopped running, or the input box never appeared.
///
/// CDXC:AgentBox 2026-10-01 WHY: the ten-second input-box wait a local agent gets is shorter than a box takes to come up (about a minute, longer with a first Claude sign-in), and text typed before that lands in agentbox's boot output or its "Paste code here" sign-in prompt. Typing into a box session therefore waits for the poller to see the box (`runtimeSettings.agentbox.created`) and then for the agent's real input box.
pub(crate) async fn wait_for_box_agent_input(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> bool {
    let deadline = Instant::now() + BOX_CREATE_WAIT;
    loop {
        let state = open_gxserver_database(paths).ok().and_then(|db| {
            DomainRepository::new(&db, server_id)
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        });
        let Some(session) = state else {
            return false;
        };
        if session
            .get("lifecycleState")
            .and_then(serde_json::Value::as_str)
            != Some("running")
        {
            return false;
        }
        if session_agentbox(&session).is_some_and(|agentbox| agentbox.created) {
            break;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(BOX_CREATE_POLL).await;
    }
    wait_for_session_chat_composer_by_ids(
        paths,
        server_id,
        project_id,
        session_id,
        SessionChatComposerWaitPolicy {
            settle_ms: 0,
            timeout_ms: BOX_INPUT_WAIT_MS,
            unknown_hold_ms: BOX_INPUT_WAIT_MS,
        },
    )
    .await
        == SessionChatComposerWait::Ready
}
