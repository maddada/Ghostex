//! The manager voice agent and Grok dictation: a spoken conversation about the user's agent
//! sessions (`/api/voice`, conversation mode) and the chat composer's dictation microphone, whose
//! recordings gxserver keeps and transcribes (`recordings.rs`, `transcribe.rs`,
//! `recordings_http.rs`; the older streaming `/api/voice?mode=dictation` is `dictation.rs`), all
//! sent to xAI or OpenAI with the keys kept on this computer. Client contract: docs/2026-10-20/voice-mode/voice-agent-client-contract.md.

mod announcements;
mod computers;
#[cfg(test)]
mod computers_tests;
mod control;
#[cfg(test)]
mod control_tests;
mod conversation;
mod dictation;
mod feature;
#[cfg(test)]
mod feature_tests;
mod focus;
#[cfg(test)]
mod focus_tests;
mod http;
mod keys;
mod loopback;
mod memory;
mod paragraphs;
mod prompt;
mod provider;
#[cfg(test)]
mod provider_tests;
pub(crate) mod question_forms;
mod recordings;
mod recordings_http;
mod registry;
mod screen;
mod sessions;
mod settings_http;
mod show;
mod socket;
mod speech;
mod talk;
#[cfg(test)]
mod talk_tests;
mod tool_runs;
mod tools;
mod transcribe;
#[cfg(test)]
mod transcribe_tests;
mod when;

pub(crate) use http::{
    handle_forget_voice_agent_memory_http, handle_read_voice_agent_http,
    handle_set_voice_agent_key_http,
};
pub(crate) use memory::recover_after_restart as recover_voice_agent_memory;
pub(crate) use recordings_http::handle_dictation_recordings_http;
pub(crate) use settings_http::handle_set_voice_agent_settings_http;
pub(crate) use socket::handle_voice;
pub(crate) use speech::handle_dictate_http;

/// Runs one voice tool as a call about all sessions on this computer runs it, with nothing kept
/// from an earlier call: for the end-to-end checks in `server/tests/voice_remote_sessions.rs`.
#[cfg(test)]
pub(crate) async fn run_tool_for_tests(
    state: &std::sync::Arc<crate::server::AppState>,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    computers::reset_for_tests();
    tools::run(state, &tools::ToolMemory::new(false), name, &args)
        .await
        .output
}

/// A warning whose `error` (kept by the log, unlike a `message` detail) holds a provider's own
/// explanation, such as an HTTP 402 body. Never pass a key, audio or transcript text here.
fn warn_error(
    state: &crate::server::AppState,
    event: &str,
    error: &str,
    details: serde_json::Value,
) {
    log_warn_error(
        &state.logger,
        &state.metadata.server_id,
        event,
        error,
        details,
    );
}

fn log_warn_error(
    logger: &crate::logging::GxserverLogger,
    server_id: &str,
    event: &str,
    error: &str,
    details: serde_json::Value,
) {
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(server_id.to_string()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: Some(details),
    });
}

/// A warning in gxserver's log (warnings are written whatever the diagnostic switches say). Never
/// pass a key, audio or transcript text here.
fn warn(state: &crate::server::AppState, event: &str, details: serde_json::Value) {
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// Logs an `error` event a Realtime provider sent (a rejected `session.update`, a refused voice, a
/// failed reply) as `event`, with the provider, the event type, code and parameter in the details and
/// the provider's sentence in `error`, so `ghostex logs --grep voice` shows why a call failed. The
/// sentence goes in `error` because the log hides a `message` detail. Returns what the event said.
fn warn_provider_error(
    state: &crate::server::AppState,
    event: &str,
    provider: provider::Provider,
    provider_event: &serde_json::Value,
) -> provider::ProviderError {
    let report = provider.describe_error(provider_event);
    log_warn_error(
        &state.logger,
        &state.metadata.server_id,
        event,
        &report.message,
        report.details.clone(),
    );
    report
}
