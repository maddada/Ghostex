use super::*;
use crate::security::Principal;

pub(crate) const TELEMETRY_PREVIEW_ENDPOINT: &str = "/api/telemetryPreview";

/*
CDXC:Telemetry 2026-08-26:
`POST /api/recordClientEvent`: the desktop app's loopback analytics ping.

Two things about this endpoint are deliberate and both matter.

It always answers 2xx for an authenticated request. The caller is
fire-and-forget on a background executor and never reads the response body, so
an error status would communicate nothing while inviting somebody to add a retry
for telemetry. A body that fails validation is dropped silently, exactly like a
server-side capture that fails validation.

Its body is NOT the `{"params": ...}` RPC envelope the domain endpoints use. It
is the flat `{"event": "...", "properties": {...}}` shape the desktop sends,
read straight off the parsed JSON (see `telemetry::client_events`, which owns
the validation and the taxonomy check).
*/
pub(crate) fn handle_record_client_event_http(
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    crate::telemetry::record_client_event(body);
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "recorded": true })),
    )
}

/*
CDXC:Telemetry 2026-11-24 (privacy overhaul, Q22):
`POST /api/telemetryPreview`: local, read-only inspection endpoint for Settings
> About ("View what's sent"). Returns the exact gate status, random install id,
and sanitized sample `heartbeat` and `settings.snapshot` payloads validated
through `telemetry::taxonomy::validate`.

Strictly read-only: never enqueues an event, never records activity, never
writes to SQLite, and never creates `analytics-install-id` on disk. Restricted
to this computer's own apps (`caller.is_owner_device()`, `full_local`).
*/
pub(crate) async fn handle_telemetry_preview_http(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
) -> RoutedResponse {
    if !caller.is_owner_device() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request(
                "Only this computer's own apps can inspect telemetry payloads.",
            ),
        );
    }
    let state = state.clone();
    let preview = tokio::task::spawn_blocking(move || build_telemetry_preview(&state)).await;
    match preview {
        Ok(payload) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, payload),
        ),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("Failed to build telemetry preview: {error}"),
            },
        ),
    }
}

fn build_telemetry_preview(state: &Arc<AppState>) -> Value {
    let mode = crate::telemetry::mode(&state.paths);
    let reason = crate::telemetry::status_reason(&state.paths);
    let enabled = mode == crate::telemetry::TelemetryMode::Full;
    let onboarding_pending = crate::telemetry::is_onboarding_held(&state.paths);
    let distinct_id = if mode == crate::telemetry::TelemetryMode::Off {
        None
    } else {
        crate::telemetry::handle()
            .map(|telemetry| telemetry.distinct_id_for_preview(mode))
            .unwrap_or_else(|| crate::telemetry::identity::read_mode_id(&state.paths, mode))
    };
    let endpoint = crate::telemetry::handle()
        .map(|telemetry| telemetry.endpoint_url().to_string())
        .unwrap_or_else(|| crate::telemetry::client::PostHogEndpoint::from_environment().batch_url);
    let queued_event_count = crate::telemetry::handle()
        .map(crate::telemetry::queue::Telemetry::queued_len)
        .unwrap_or(0);

    let weekly_count = match mode {
        crate::telemetry::TelemetryMode::Full => {
            crate::telemetry::build_weekly_count_preview_event(distinct_id.as_deref(), true)
        }
        crate::telemetry::TelemetryMode::WeeklyCountOnly => {
            crate::telemetry::build_weekly_count_preview_event(distinct_id.as_deref(), false)
        }
        crate::telemetry::TelemetryMode::Off => None,
    };

    let (heartbeat, settings_snapshot) = if mode == crate::telemetry::TelemetryMode::Full {
        let install_created_at = read_install_created_at(&state.paths);
        let heartbeat_snapshot =
            super::telemetry_tasks::collect_heartbeat_snapshot(state, &install_created_at);
        let profile_snapshot = heartbeat_snapshot
            .as_ref()
            .map(|snapshot| {
                crate::telemetry::ProfileSnapshot::from_counts(
                    snapshot.project_count,
                    snapshot.default_agent,
                )
            })
            .or_else(|| crate::telemetry::handle().map(|telemetry| telemetry.read_profile()))
            .unwrap_or_default();

        let heartbeat = heartbeat_snapshot.as_ref().and_then(|snapshot| {
            crate::telemetry::build_preview_event(
                &state.paths,
                &profile_snapshot,
                distinct_id.as_deref(),
                crate::telemetry::taxonomy::EVENT_HEARTBEAT,
                &crate::telemetry::heartbeat::heartbeat_properties(snapshot),
            )
        });

        let settings = crate::shared_settings_file::read_shared_settings(&state.paths);
        let facts = super::telemetry_tasks::collect_settings_snapshot_facts(state, false);
        let settings_snapshot = crate::telemetry::build_preview_event(
            &state.paths,
            &profile_snapshot,
            distinct_id.as_deref(),
            crate::telemetry::taxonomy::EVENT_SETTINGS_SNAPSHOT,
            &crate::telemetry::settings_snapshot::snapshot_properties(settings.as_ref(), &facts),
        );
        (heartbeat, settings_snapshot)
    } else {
        (None, None)
    };

    json!({
        "enabled": enabled,
        "mode": mode.as_str(),
        "reason": reason.as_str(),
        "onboardingPending": onboarding_pending,
        "distinctId": distinct_id,
        "endpoint": endpoint,
        "queuedEventCount": queued_event_count,
        "weeklyCount": weekly_count,
        "heartbeat": heartbeat,
        "settingsSnapshot": settings_snapshot,
    })
}

fn read_install_created_at(paths: &crate::paths::GxserverPaths) -> String {
    std::fs::read_to_string(&paths.identity_file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| {
            value
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string())
}
