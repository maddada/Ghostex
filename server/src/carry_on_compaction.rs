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

/// CDXC:Coordinators 2026-11-15 DECISION:
/// User: "regarding compacting when returning to a conversation, can we do this also to worker
/// threads under a coordinator so we lower token usage? (enabled by default)". When a coordinator's
/// message reaches one of its own threads (`ghostex agents send`, a follow-up, a reopened done
/// thread) and that worker has sat idle longer than its prompt cache lives, with a last known
/// context at or above Compact chats above, on an agent with a compact command, Ghostex types the
/// compact command first and holds the coordinator's message in the worker's queue, which types it
/// once the compaction's turn ends. The send's answer says so (`compactedFirst`), and the CLI
/// prints it, so the coordinator does not take the wait for a lost message; the worker's chat shows
/// the /compact before the message. Settings > Agents > Claude Code "Compact workers before a
/// coordinator returns to them", on by default, turns it off. Messages a person types are never
/// touched: only a message signed by the thread's own coordinator counts.
/// The cache life is the agent's real one: a Claude session's last cache write in its transcript
/// says whether it used the 1-hour cache (`ephemeral_1h_input_tokens`) or the 5-minute one
/// (`ephemeral_5m_input_tokens`), and with no write to read it counts as 1 hour (a subscription's
/// cache, so a worker is not compacted early); Codex's cached prefix lasts about a day (the 24 hours
/// the Codex compact-then-send hint uses, packages/gx-chat-core/src/composer/compact_hint.rs). Other
/// agents' cache life is not known, so they are never compacted for this.
/// SEE-ALSO: server/src/session_chat_send/send_reopen.rs (where the send is held),
/// server/src/ghostex_cli/agents/delivery.rs (what the CLI prints).
pub(crate) const COMPACT_WORKERS_SETTING: &str = "compactWorkersBeforeReturn";

const FIVE_MINUTES_MS: i64 = 5 * 60_000;
const ONE_HOUR_MS: i64 = 60 * 60_000;
const ONE_DAY_MS: i64 = 24 * ONE_HOUR_MS;
/// How much of a Claude transcript's end is read for its last cache write.
const TRANSCRIPT_TAIL_BYTES: u64 = 512 * 1024;

/// Whether "Compact workers before a coordinator returns to them" is on (on by default).
pub(crate) fn compact_workers_enabled() -> bool {
    crate::session_lifecycle::read_sidebar_settings(&crate::paths::get_gxserver_paths(None))
        .and_then(|settings| {
            settings
                .get(COMPACT_WORKERS_SETTING)
                .and_then(Value::as_bool)
        })
        .unwrap_or(true)
}

/// A worker Ghostex compacts before its coordinator's message.
#[derive(Clone, Debug)]
pub(crate) struct WorkerCompaction {
    pub(crate) command: &'static str,
    pub(crate) tokens: u64,
    pub(crate) idle_ms: i64,
    pub(crate) cache_life_ms: i64,
}

impl WorkerCompaction {
    /// The send answer's `compactedFirst`.
    pub(crate) fn to_value(&self) -> Value {
        serde_json::json!({
            "tokens": self.tokens,
            "idleMinutes": self.idle_ms / 60_000,
            "cacheLifeMinutes": self.cache_life_ms / 60_000,
        })
    }
}

/// The session's prompt cache life, when Ghostex knows it for its agent.
fn cache_life_ms(repository: &DomainRepository<'_>, session: &Value) -> Option<i64> {
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .or_else(|| crate::session_chat_follower::session_chat_display_agent_for_session(session));
    match crate::session_chat_options::session_chat_option_agent(agent.as_deref())? {
        crate::session_chat_options::SessionChatOptionAgent::Codex => Some(ONE_DAY_MS),
        crate::session_chat_options::SessionChatOptionAgent::Claude => {
            let paths = crate::paths::get_gxserver_paths(None);
            let path = crate::session_chat_options::claude_session_transcript_path(
                repository,
                &crate::session_chat_options::session_chat_hook_state_directory(&paths),
                session["projectId"].as_str()?,
                session["sessionId"].as_str()?,
            );
            Some(
                path.and_then(|path| claude_last_cache_life_ms(std::path::Path::new(&path)))
                    .unwrap_or(ONE_HOUR_MS),
            )
        }
        _ => None,
    }
}

/// The cache life of a Claude transcript's last cache write: 1 hour when it wrote to the 1-hour
/// cache, 5 minutes when it wrote to the 5-minute one.
fn claude_last_cache_life_ms(path: &std::path::Path) -> Option<i64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(
        length.saturating_sub(TRANSCRIPT_TAIL_BYTES),
    ))
    .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    let tail = String::from_utf8_lossy(&tail);
    const NEEDLE: &str = "\"cache_creation\":";
    let mut rest: &str = &tail;
    let mut last = None;
    while let Some(at) = rest.find(NEEDLE) {
        rest = &rest[at + NEEDLE.len()..];
        let Some(end) = rest.find('}') else {
            break;
        };
        if let Ok(write) = serde_json::from_str::<Value>(&rest[..=end]) {
            let tokens = |key: &str| write.get(key).and_then(Value::as_u64).unwrap_or(0);
            if tokens("ephemeral_1h_input_tokens") > 0 {
                last = Some(ONE_HOUR_MS);
            } else if tokens("ephemeral_5m_input_tokens") > 0 {
                last = Some(FIVE_MINUTES_MS);
            }
        }
    }
    last
}

/// When the session's agent last stopped working, while it is idle now.
fn idle_since_ms(session: &Value) -> Option<i64> {
    let activity = session.pointer("/runtimeSettings/agentActivity")?;
    if activity["activity"] == "working" {
        return None;
    }
    activity["lastChangedAt"]
        .as_str()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis())
}

/// BLOCKING. Whether the coordinator's message `text` to the session `key` should wait behind a
/// compaction: it is signed by the thread's own coordinator, the worker has been idle longer than
/// its prompt cache lives, and it is big enough to compact.
pub(crate) fn plan_worker_compaction(
    repository: &DomainRepository<'_>,
    key: &(String, String),
    text: &str,
) -> Option<WorkerCompaction> {
    // Only an agent's signed message can be a coordinator's: a person's message stops here.
    let reply_to = crate::coordinators::split_agent_message(text)?.reply_to;
    if !compact_workers_enabled() {
        return None;
    }
    let mut parts = reply_to.rsplit(':');
    let (sender_session, sender_project) = (parts.next()?, parts.next()?);
    let thread = crate::coordinators::read_thread(repository.db, &key.0, &key.1).ok()??;
    if thread.coordinator_project_id != sender_project
        || thread.coordinator_session_id != sender_session
    {
        return None;
    }
    let session = repository.get_session(&key.0, &key.1).ok()??;
    let idle_ms = chrono::Utc::now().timestamp_millis() - idle_since_ms(&session)?;
    let cache_life_ms = cache_life_ms(repository, &session)?;
    if idle_ms <= cache_life_ms {
        return None;
    }
    let command = compact_command(&session)?;
    let tokens = worth_compacting(repository, &session)?;
    Some(WorkerCompaction {
        command,
        tokens,
        idle_ms,
        cache_life_ms,
    })
}
