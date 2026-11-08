//! The manager voice agent and Grok dictation: a spoken conversation about the user's agent
//! sessions (`/api/voice`, conversation mode) and the chat composer's dictation microphone, whose
//! recordings gxserver keeps and transcribes (`recordings.rs`, `transcribe.rs`,
//! `recordings_http.rs`; the older streaming `/api/voice?mode=dictation` is `dictation.rs`), all
//! sent to xAI or OpenAI with the keys kept on this computer. Client contract: docs/2026-10-20/voice-mode/voice-agent-client-contract.md.

mod announcements;
mod approvals;
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
pub(crate) use speech::{dictate_uses_hex, handle_dictate_http};

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

/// Runs one voice tool as a conversation does, for a client that opened the call with `token`
/// (`None`: the token file): the tool's output, and the text of every announcement the
/// conversation would get afterwards.
#[cfg(test)]
pub(crate) async fn run_tool_in_call_for_tests(
    state: &std::sync::Arc<crate::server::AppState>,
    token: Option<&str>,
    name: &str,
    args: serde_json::Value,
) -> (
    serde_json::Value,
    tokio::sync::mpsc::UnboundedReceiver<(String, Option<String>)>,
) {
    computers::reset_for_tests();
    let memory = std::sync::Arc::new(tools::ToolMemory::new(false));
    if let Some(token) = token {
        memory.set_caller_token(token.to_string());
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (texts, announced) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(input) = rx.recv().await {
            if let conversation::Input::Announce(announcements) = input {
                for announcement in announcements {
                    let _ = texts.send((announcement.text, announcement.model_text));
                }
            }
        }
    });
    let outcome = approvals::run_in_call(state.clone(), memory, name.to_string(), args, tx).await;
    (outcome.output, announced)
}

/// One voice tool call's requests as the client holding `token` (`None`: the token file), for
/// the end-to-end checks in `server/tests/voice_caller_token.rs`.
#[cfg(test)]
pub(crate) struct VoiceCallerForTests {
    caller: loopback::ToolCaller,
    pending: std::sync::Mutex<Option<loopback::PendingApproval>>,
}

#[cfg(test)]
impl VoiceCallerForTests {
    pub(crate) fn new(token: Option<&str>) -> Self {
        Self {
            caller: loopback::ToolCaller::new(token.map(str::to_string)),
            pending: Default::default(),
        }
    }

    /// One request inside the tool call; an approval it ran into is kept for [`Self::decide`].
    pub(crate) async fn call(
        &self,
        state: &std::sync::Arc<crate::server::AppState>,
        path: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let result = loopback::with_caller(
            self.caller.clone(),
            loopback::call(state, path, params, std::time::Duration::from_secs(10)),
        )
        .await;
        if let Some(pending) = self.caller.take_pending() {
            *self.pending.lock().unwrap() = Some(pending);
        }
        result
    }

    /// The id of the approval the last call ran into.
    pub(crate) fn pending_id(&self) -> Option<String> {
        self.pending
            .lock()
            .unwrap()
            .as_ref()
            .map(|pending| pending.id.clone())
    }

    /// Waits as a conversation does until the user decided, and on an approval lets the next
    /// call send it: `approved`, `denied`, `lapsed`.
    pub(crate) async fn decide(
        &self,
        state: &std::sync::Arc<crate::server::AppState>,
    ) -> &'static str {
        let pending = self
            .pending
            .lock()
            .unwrap()
            .clone()
            .expect("a pending approval");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        match approvals::wait_for_decision(state, &self.caller, &pending, &tx).await {
            approvals::Decision::Approved => {
                self.caller.approve(&pending);
                "approved"
            }
            approvals::Decision::Denied => "denied",
            approvals::Decision::Lapsed => "lapsed",
            approvals::Decision::Gone => "gone",
        }
    }
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
