//! The desktop writes scenario-gated diagnostic files; the browser has no disk, so the same calls go to the console at debug level.
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(crate) enum GpuiSupportLog {
    SidebarRefresh,
    TerminalFocus,
    SessionChat,
    ProjectBoard,
}

pub(crate) fn append(log: GpuiSupportLog, event: &str, details: Value) {
    log::debug!("{log:?} {event} {details}");
}

/// Always kept on the desktop (a chat send, native_chat/send_watch.rs); the console at info level.
pub(crate) fn append_always(log: GpuiSupportLog, event: &str, details: Value) {
    log::info!("{log:?} {event} {details}");
}

/// Scenario-gated on the desktop; the console has no scenarios, so the call is logged like `append`.
pub(crate) fn append_for_scenario(
    log: GpuiSupportLog,
    _scenario_id: &str,
    event: &str,
    details: Value,
) {
    append(log, event, details);
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GpuiDiagnosticScenario {
    SidebarRefresh,
    SessionChat,
    TerminalFocus,
}

/// Scenario-gated disk logging has no disk to write to here, so no scenario is ever on.
pub(crate) fn scenario_enabled(_scenario: GpuiDiagnosticScenario) -> bool {
    false
}

/// The desktop's always-on `gpui-help-chat-timing.log`; the page logs the steps to the console.
pub(crate) fn append_help_chat_timing(event: &str, details: Value) {
    log::info!("help chat {event} {details}");
}
