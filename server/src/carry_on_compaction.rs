//! Whether a session is compacted before Ghostex carries it on after a break: a restart cut it
//! off, or its usage limit reset.
//!
//! CDXC:AgentProviders 2026-11-14 DECISION:
//! GitHub #608, from the compaction page (i608-fix/compact.html): every carry-on in that fix
//! happens after the prompt cache expired, so its first turn re-reads the whole conversation at
//! full price. The user picked "Above a size" (Q3237): a session whose last known context is at
//! or above one size, and whose agent has a compact command, is compacted first; below it, or
//! without a command, it carries on as is. On the cards that ask before carrying on, a Compact
//! first checkbox shows it, ticked by default for the sessions at or above the size, with an (i)
//! whose tooltip says why (Q3238: "checkbox to compact ones above x; put an i icon in a circle so
//! we can hover to see a tooltip saying it's recommended to compact to save a lot on usage when
//! resuming sessions"). The size is Claude's "Only above" (200K by default), one number for every
//! agent, renamed to cover both idle compaction and carrying on (Q3239). Carry-ons nobody clicks
//! (the restart switch on, a limit wait that continues by itself) follow the same size rule.
//! SEE-ALSO: server/src/agents/claude_idle_compaction.rs (the size), server/src/accounts/recovery.rs
//! (limit waits), server/src/coordinators/stopped_by_restart.rs (threads a restart cut off),
//! packages/gx-chat-core/src/questions/account_wait.rs (the limit card's checkbox).

use serde_json::Value;

use crate::domain::DomainRepository;

/// The (i) tooltip beside every Compact first checkbox.
pub(crate) const COMPACT_HINT: &str = "Compacting first is recommended: it saves a lot of usage when resuming a session, because the first turn after a break re-reads the whole conversation, and a compacted one is a fraction of the size.";

/// The size at and above which a session is compacted before it carries on, in tokens.
pub(crate) fn threshold() -> u64 {
    crate::agents::claude_idle_compaction::ClaudeCompaction::read().min_tokens
}

/// The command that compacts this session's agent, when Ghostex knows one.
pub(crate) fn compact_command(session: &Value) -> Option<&'static str> {
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .or_else(|| crate::session_chat_follower::session_chat_display_agent_for_session(session));
    crate::session_chat_options::is_session_chat_activity_command_text(agent.as_deref(), "/compact")
        .then_some("/compact")
}

/// The session's last known context size, from what the agent left on disk (Claude's status line
/// file); `None` when nothing says.
pub(crate) fn context_tokens(repository: &DomainRepository<'_>, session: &Value) -> Option<u64> {
    let project_id = session["projectId"].as_str()?;
    let session_id = session["sessionId"].as_str()?;
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .or_else(|| crate::session_chat_follower::session_chat_display_agent_for_session(session));
    let paths = crate::paths::get_gxserver_paths(None);
    let hook_state_directory =
        crate::session_chat_options::session_chat_hook_state_directory(&paths);
    let detected = crate::session_chat_options::detect_session_chat_stored_options(
        repository,
        &hook_state_directory,
        project_id,
        session_id,
        agent.as_deref(),
    )?;
    let usage = detected.selection.context_usage?;
    usage
        .used_tokens
        .or_else(|| Some(u64::from(usage.used_percentage?) * usage.window_size? / 100))
}

/// The session's context size when it is worth compacting before carrying on: at or above the
/// size, on an agent with a compact command.
pub(crate) fn worth_compacting(repository: &DomainRepository<'_>, session: &Value) -> Option<u64> {
    compact_command(session)?;
    context_tokens(repository, session).filter(|tokens| *tokens >= threshold())
}

/// "312K" for 312,400 tokens.
pub(crate) fn short_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else {
        format!("{}K", (tokens + 500) / 1_000)
    }
}

/// Types `text` (the compact command, or a carry-on message) into the session the way a coordinator's message goes: typed now,
/// taken by a busy agent at its next input boundary, a sleeping session woken for it; whatever is
/// sent after it waits in the queue until the compaction's turn ends.
pub(crate) async fn send_like_coordinator(
    state: &crate::server::AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "params": {
            "projectId": project_id,
            "sessionId": session_id,
            "text": text,
            "sendRequestId": uuid::Uuid::new_v4().to_string(),
        }
    });
    let routed = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        uuid::Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    if routed.response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "the send was refused ({})",
            routed.response.status()
        ))
    }
}
