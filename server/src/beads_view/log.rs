//! One log line per bd process the Beads view and the Board start, written only while the
//! Project board diagnostic log is on.

use serde_json::json;

use crate::logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel};

fn logger() -> &'static GxserverLogger {
    static LOGGER: std::sync::OnceLock<GxserverLogger> = std::sync::OnceLock::new();
    LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)))
}

/// `why` says which path started it (`discovery`, `refresh`, `read`, `action`, `sync`).
pub(crate) fn bd_call(why: &str, action: &str) {
    let _ = logger().log_routine(
        DiagnosticLogScenario::BeadsCalls,
        GxserverLogInput {
            level: LogLevel::Info,
            event: "beadsView.bdCall".to_string(),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({ "why": why, "action": action })),
        },
    );
}

/// A problem worth keeping whatever the diagnostics switches say.
pub(crate) fn warn(event: &str, message: &str) {
    let _ = logger().log(GxserverLogInput {
        level: LogLevel::Warn,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}
