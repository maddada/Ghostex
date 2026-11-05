//! Server routes: the authenticated health read, client events, logs, Tailcat, remote access, extensions, feedback, and the stop / stop-all control plane.

use axum::http::StatusCode;
use serde_json::json;

use crate::{
    extensions::handle_extensions_http,
    feedback::{handle_feedback_http, handle_start_feedback_agent_http},
    protocol::rpc_success,
    remote_access::handle_remote_access_http,
    tailcat::handle_tailcat_http,
};

use super::*;

pub(super) async fn route_control_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/health/server" => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            create_authenticated_health(&state),
        ),
        "/api/recordClientEvent" => {
            handle_record_client_event_http(endpoint.path, request_id, &body_json)
        }
        "/api/queryLogs" => handle_query_logs_http(&state, endpoint.path, request_id, &body_json),
        "/api/draftFeedback" | "/api/sendFeedback" => {
            handle_feedback_http(endpoint.path, request_id, &body_json).await
        }
        "/api/startFeedbackAgent" => {
            handle_start_feedback_agent_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/tailcatStatus" | "/api/updateTailcatState" | "/api/installTailcat" => {
            handle_tailcat_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/remoteAccessStatus"
        | "/api/enableSshAccess"
        | "/api/remotePairingCode"
        | "/api/pairedDevices"
        | "/api/removePairedDevice"
        | "/api/pairDevice"
        | "/api/pairedDeviceSeen" => {
            handle_remote_access_http(&state, endpoint.path, request_id, &body_json).await
        }
        path if crate::connected_computers::CONNECTED_COMPUTER_ENDPOINTS.contains(&path) => {
            crate::connected_computers::handle_connected_computers_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
        }
        "/api/appUpdateStatus"
        | "/api/appUpdateCheck"
        | "/api/appUpdateInstall"
        | "/api/appUpdateSetAuto" => {
            crate::app_update::handle_app_update_http(&state, endpoint.path, request_id, &body_json)
                .await
        }
        "/api/updates" => {
            crate::updates::handle_updates_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/listExtensions"
        | "/api/extensionsCatalog"
        | "/api/installExtension"
        | "/api/uninstallExtension"
        | "/api/updateExtensionState"
        | "/api/startExtension"
        | "/api/stopExtension"
        | "/api/extensionStatus"
        | "/api/extensionBadge" => {
            handle_extensions_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                token_extension_id,
            )
            .await
        }
        "/api/control/stop" => {
            broadcast_server_stopping(&state);
            let response = routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(request_id, json!({})),
            );
            let _ = state.shutdown_tx.send(());
            response
        }
        "/api/control/stopAll" => {
            broadcast_server_stopping(&state);
            /*
            CDXC:SessionSleep 2026-08-28:
            Kill the tracked zmx sessions BEFORE answering and before the
            shutdown broadcast, so the reported counts are real and the kills
            cannot race the control plane's own teardown. The kill loop runs
            zmx subprocesses per session, so it leaves the async executor.
            */
            let stop_all_state = state.clone();
            let counts = tokio::task::spawn_blocking(move || {
                zmx_http::kill_all_tracked_zmx_sessions(&stop_all_state)
            })
            .await
            .unwrap_or(zmx_http::ZmxStopAllCounts {
                attempted: 0,
                failed: 0,
                killed: 0,
                skipped: 0,
            });
            // An isolated profile leaves no login-recovery agent behind (CDXC:ServerDaemon 2026-11-06 in macos_login_recovery.rs).
            #[cfg(target_os = "macos")]
            if let Ok(port) = crate::config::read_selected_local_api_port() {
                if crate::server::macos_login_recovery::is_isolated_profile(port) {
                    if let Err(error) = crate::server::macos_login_recovery::uninstall(port) {
                        eprintln!("remove gxserver login-session recovery agent failed: {error:#}");
                    }
                }
            }
            let response = routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({
                        "attemptedSessions": counts.attempted,
                        "failedSessions": counts.failed,
                        "killedSessions": counts.killed,
                        "skippedSessions": counts.skipped,
                    }),
                ),
            );
            let _ = state.shutdown_tx.send(());
            response
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
            })
        }
    })
}
