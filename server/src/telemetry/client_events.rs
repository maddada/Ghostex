/*
CDXC:Telemetry 2026-08-26:
`POST /api/recordClientEvent` — the desktop app's loopback ping and, since
2026-09-03, the mobile app's hello (forwarded by `ghostex client-hello` over
SSH). This is the ONLY way a client process contributes to analytics. The
desktop, web, and mobile apps never talk to PostHog; they hand gxserver a
two-field body and gxserver decides whether anything is sent.

The trust model here is "none". The body is parsed defensively, only the
events listed in `CLIENT_PING_EVENTS` are accepted, and every property is
re-validated against the same taxonomy every server-side emitter goes through.
Anything that does not fit is dropped silently with a debug log — the caller is
fire-and-forget and never reads the response, so there is nothing useful to tell
it, and an error status would only tempt someone into adding a retry for
telemetry.
*/

use serde_json::Value;

use super::{
    capture::{self, ClientPlatform},
    taxonomy::{self, EVENT_APP_LAUNCHED, EVENT_CLIENT_CONNECTED, EVENT_SURFACE_OPENED},
};

/// Handle one `{"event": "...", "properties": {...}}` body. Never fails: the
/// endpoint always answers 2xx.
pub fn record_client_event(body: &Value) {
    let Some(event) = body.get("event").and_then(Value::as_str).map(str::trim) else {
        super::debug_log("recordClientEvent body has no event name".to_string());
        return;
    };
    if !taxonomy::is_client_ping_event(event) {
        super::debug_log(format!(
            "recordClientEvent rejected {event}: not a client ping event"
        ));
        return;
    }
    let properties = body.get("properties");
    match event {
        EVENT_APP_LAUNCHED => record_app_launched(properties),
        EVENT_SURFACE_OPENED => record_surface_opened(properties),
        EVENT_CLIENT_CONNECTED => record_client_connected(properties),
        _ => {}
    }
}

fn record_app_launched(properties: Option<&Value>) {
    /*
    `client` is pinned to `desktop` here rather than read from the body: this
    endpoint IS the desktop ping, and accepting the client kind from the caller
    would let a mis-tagged (or hand-crafted) request pollute the one dimension
    the event exists to report.
    */
    let Some(app_version) = properties
        .and_then(|properties| properties.get("app_version"))
        .and_then(Value::as_str)
        .and_then(taxonomy::normalize_version_string)
    else {
        super::debug_log("recordClientEvent app.launched has no usable app_version".to_string());
        return;
    };
    capture::app_launched(app_version);
}

fn record_surface_opened(properties: Option<&Value>) {
    let Some(surface) = properties
        .and_then(|properties| properties.get("surface"))
        .and_then(Value::as_str)
        .and_then(|surface| taxonomy::match_enum(taxonomy::SURFACES, surface))
    else {
        super::debug_log("recordClientEvent surface.opened has no known surface".to_string());
        return;
    };
    capture::surface_opened(surface);
}

/*
CDXC:Telemetry 2026-09-03:
The mobile hello. `client` is pinned to `mobile` for the same reason
`app.launched` pins `desktop`: the web app's attach is observed server-side at
`/api/webBootstrap`, so the only client that legitimately reaches this path is
the phone, via `ghostex client-hello`. The OS must be one of the two mobile
platforms; a body claiming anything else is dropped whole rather than sent as a
mobile event with a desktop OS on it. Versions are optional and clamped, so an
older app that sends neither still counts as a mobile attach.
*/
fn record_client_connected(properties: Option<&Value>) {
    let Some(os) = properties
        .and_then(|properties| properties.get("client_os"))
        .and_then(Value::as_str)
        .and_then(|os| taxonomy::match_enum(taxonomy::MOBILE_PLATFORMS, os))
    else {
        super::debug_log("recordClientEvent client.connected has no mobile client_os".to_string());
        return;
    };
    let version = |key: &str| {
        properties
            .and_then(|properties| properties.get(key))
            .and_then(Value::as_str)
            .and_then(taxonomy::normalize_version_string)
    };
    capture::client_connected(
        "mobile",
        ClientPlatform {
            os: Some(os),
            os_version: version("client_os_version"),
            app_version: version("client_app_version"),
        },
    );
}
