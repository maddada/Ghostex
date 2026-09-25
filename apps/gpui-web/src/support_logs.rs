//! The desktop writes scenario-gated diagnostic files; the browser has no disk, so the same calls go to the console at debug level.
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub(crate) enum GpuiSupportLog {
    SidebarRefresh,
    TerminalFocus,
    SessionChat,
}

pub(crate) fn append(log: GpuiSupportLog, event: &str, details: Value) {
    log::debug!("{log:?} {event} {details}");
}

/// Scenario-gated on the desktop; the console has no scenarios, so the call is logged like `append`.
pub(crate) fn append_for_scenario(log: GpuiSupportLog, _scenario_id: &str, event: &str, details: Value) {
    append(log, event, details);
}

pub(crate) fn temporary_epoch_ms() -> u64 {
    js_sys::Date::now() as u64
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

pub(crate) fn append_temporary(log: GpuiSupportLog, event: &str, details: Value) {
    append(log, event, details);
}

/// The desktop records the shape of dictated text for a reproduction; nothing is recorded here.
pub(crate) fn temporary_fluid_voice_text_shape(_text: &str) -> Value {
    Value::Null
}
