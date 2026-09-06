use serde_json::{json, Value};

use crate::logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel};

/// CDXC:Drafts 2026-09-05 SEE-ALSO:
/// packages/core-ui/chat/session-chat-draft-diagnostics.ts uses the same UTF-8 fingerprint for correlating restored fragments with durable saves.
pub(crate) fn fingerprint(value: &str) -> Value {
    let hash = value.bytes().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    });
    json!({ "chars": value.encode_utf16().count(), "bytes": value.len(), "fingerprint": format!("{hash:08x}") })
}

pub(crate) fn log(
    logger: &GxserverLogger,
    phase: &str,
    project_id: &str,
    session_id: &str,
    details: Value,
) {
    let _ = logger.log_routine(
        DiagnosticLogScenario::SessionChatDrafts,
        GxserverLogInput {
            level: LogLevel::Debug,
            event: format!("sessionChat.draft.{phase}"),
            server_id: None,
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(
                json!({ "projectId": project_id, "sessionId": session_id, "details": details }),
            ),
        },
    );
}
