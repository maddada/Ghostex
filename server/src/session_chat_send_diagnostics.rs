use std::{fs, io::Write, sync::Mutex};

use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

#[derive(Clone)]
pub(crate) struct SendFailureReason {
    pub code: String,
    pub message: String,
}

static WRITER: Mutex<()> = Mutex::new(());

/// CDXC:SessionChat 2026-09-16 DECISION:
/// User: always log the reason a send failed and the terminal's last page so a reported local time can identify the failure.
/// Keep this content-bearing diagnostic separate from the redacted support log; draft-save failures also prevent Send from reaching the terminal.
pub(crate) async fn record_response(
    state: &AppState,
    request_id: &str,
    body: &Value,
    response: &mut RoutedResponse,
) {
    if !matches!(
        response.endpoint_path.as_deref(),
        Some(
            "/api/sendSessionChatMessage"
                | "/api/setSessionChatDraft"
                | "/api/queueSessionChatPrompt"
                | "/api/sendSessionChatQueuedPrompt"
        )
    ) {
        return;
    }
    let Some(reason) = response.response.extensions().get::<SendFailureReason>() else {
        return;
    };
    if reason.code == "sendCancelled" {
        return;
    }
    let params = crate::domain::read_domain_rpc_params(body).unwrap_or_default();
    let mut entry = json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "event": "sessionChatSendFailure",
        "requestId": request_id,
        "serverId": state.metadata.server_id,
        "endpoint": response.endpoint_path,
        "statusCode": response.response.status().as_u16(),
        "projectId": params.get("projectId"),
        "sessionId": params.get("sessionId"),
        "code": reason.code,
        "message": reason.message,
        "draftVersion": params.get("draftVersion"),
        "submitted": params.get("text").and_then(Value::as_str)
            .map(crate::session_chat_draft_diagnostics::fingerprint),
    });
    // Persist the reason before doing any capture, including when capture itself fails.
    persist(state, &entry);
    entry["event"] = json!("sessionChatSendFailureTerminal");
    match crate::session_chat_send::resolve_session_chat_send_target(
        state,
        &params,
        "sendDiagnostics",
    ) {
        Ok(target) => {
            entry["zmxName"] = json!(target.zmx_name);
            entry["agentSessionId"] = target
                .session
                .get("agentSessionId")
                .cloned()
                .unwrap_or(Value::Null);
            let name = target.zmx_name;
            match tokio::task::spawn_blocking(move || {
                crate::zmx::read_zmx_session_screen_capture(&name)
            })
            .await
            {
                Ok(Ok(capture)) => {
                    entry["terminal"] = json!({
                        "capturedAt": chrono::Utc::now().to_rfc3339(),
                        "captured": true,
                        "truncated": capture.truncated,
                        "text": capture.text,
                    });
                }
                Ok(Err(error)) => entry["captureError"] = json!(error),
                Err(error) => entry["captureError"] = json!(error.to_string()),
            }
        }
        Err(error) => entry["captureError"] = json!(error.message),
    }
    persist(state, &entry);
}

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: log every break in sending with the terminal screen, so a problem they report can be diagnosed from the log alone. A send that had to clear something first (close a Claude panel with Escape, wake a sleeping session, hold a message for an agent still starting) writes one line here next to the failures, with the screen tail it acted on.
pub(crate) fn record_send_recovery(
    state: &AppState,
    event: &str,
    project_id: &str,
    session_id: &str,
    reason: &str,
    screen_tail: &[String],
) {
    persist(
        state,
        &json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "event": event,
            "serverId": state.metadata.server_id,
            "projectId": project_id,
            "sessionId": session_id,
            "reason": reason,
            "terminal": { "tail": screen_tail.join("\n") },
        }),
    );
}

fn persist(state: &AppState, entry: &Value) {
    if let Err(error) = append(&state.paths.logs_dir, entry) {
        eprintln!("session chat send diagnostic could not be written: {error}");
        let _ = state.logger.log(crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Error,
            event: "sessionChatSendDiagnosticWriteFailed".into(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: entry
                .get("requestId")
                .and_then(Value::as_str)
                .map(str::to_string),
            client: None,
            duration_ms: None,
            error: Some(error.to_string()),
            details: Some(json!({ "code": entry.get("code"), "message": entry.get("message") })),
        });
    }
}

fn append(directory: &std::path::Path, entry: &Value) -> std::io::Result<()> {
    let _guard = WRITER.lock().unwrap_or_else(|error| error.into_inner());
    fs::create_dir_all(directory)?;
    let path = directory.join("session-chat-send-failures.jsonl");
    let line = serde_json::to_string(entry)?;
    if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) + line.len() as u64 > 5 * 1024 * 1024 {
        for index in (1..=3).rev() {
            let source = if index == 1 {
                path.clone()
            } else {
                path.with_extension(format!("jsonl.{}", index - 1))
            };
            let destination = path.with_extension(format!("jsonl.{index}"));
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            if source.exists() {
                fs::rename(source, destination)?;
            }
        }
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    writeln!(options.open(path)?, "{line}")
}
