use super::*;

/*
CDXC:AnonymousAnalytics 2026-08-26:
`POST /api/recordClientEvent` — the desktop app's loopback analytics ping.

Two things about this endpoint are deliberate and both matter.

It always answers 2xx for an authenticated request. The caller is
fire-and-forget on a background executor and never reads the response body, so
an error status would communicate nothing while inviting somebody to add a retry
for telemetry. A body that fails validation is dropped silently, exactly like a
server-side capture that fails validation.

Its body is NOT the `{"params": …}` RPC envelope the domain endpoints use. It is
the flat `{"event": "…", "properties": {…}}` shape the desktop sends, read
straight off the parsed JSON — see `telemetry::client_events`, which owns the
validation and the taxonomy check.
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
