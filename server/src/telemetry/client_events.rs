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
    extensions::{self, ExtensionRef},
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
    let Some((extension, client)) = surface_opened_details(surface, properties) else {
        super::debug_log("recordClientEvent surface.opened has invalid details".to_string());
        return;
    };
    capture::surface_opened(surface, extension, client);
}

/*
CDXC:Telemetry 2026-10-27:
The two details a `surface.opened` may carry beyond its surface. `extension` is
read only for the `extension` surface, and the raw id the client sent is turned
into a built-in id, an official Store id, or `custom` right here, so the id a
user gave their own extension or view stops at gxserver. `client` defaults to
none (older desktops sent none); a phone's screens (`mobile_*`) must say
`mobile` and the desktop's must not, so one client cannot report the other's
views.
*/
pub(super) fn surface_opened_details(
    surface: &'static str,
    properties: Option<&Value>,
) -> Option<(Option<ExtensionRef>, Option<&'static str>)> {
    let text = |key: &str| {
        properties
            .and_then(|properties| properties.get(key))
            .and_then(Value::as_str)
    };
    let extension = if surface == "extension" {
        let id = text("extension").unwrap_or("");
        let mut resolved = extensions::resolve_extension(id);
        // A Store extension installed since the last heartbeat is in the refreshed catalog cache.
        if resolved == ExtensionRef::Custom {
            if let Some(telemetry) = super::queue::handle() {
                extensions::load_official_store_ids(&telemetry.paths().extensions_dir);
                resolved = extensions::resolve_extension(id);
            }
        }
        Some(resolved)
    } else {
        None
    };
    let client = match text("client") {
        None => None,
        Some(client) => Some(taxonomy::match_enum(taxonomy::SURFACE_CLIENTS, client)?),
    };
    let mobile_surface = surface.starts_with("mobile_");
    if mobile_surface != (client == Some("mobile")) {
        return None;
    }
    Some((extension, client))
}

/// CDXC:Telemetry 2026-09-06 WHY:
/// The standalone `ghostex web` bootstrap forwards its hello through this endpoint because analytics remain owned by gxserver.
/// Older mobile callers omit `client`, so retain their mobile classification and OS validation while accepting an explicit web client with a browser-derived platform.
fn record_client_connected(properties: Option<&Value>) {
    let client = match properties.and_then(|value| value.get("client")) {
        None => "mobile",
        Some(Value::String(client)) if client == "mobile" => "mobile",
        Some(Value::String(client)) if client == "web" => "web",
        _ => return,
    };
    let platforms = if client == "web" {
        taxonomy::CLIENT_PLATFORMS
    } else {
        taxonomy::MOBILE_PLATFORMS
    };
    let raw_os = properties.and_then(|value| value.get("client_os"));
    let os = raw_os
        .and_then(Value::as_str)
        .and_then(|os| taxonomy::match_enum(platforms, os));
    if os.is_none() && (client == "mobile" || raw_os.is_some_and(|value| !value.is_null())) {
        super::debug_log("recordClientEvent client.connected has no valid client_os".to_string());
        return;
    }
    let version = |key: &str| {
        properties
            .and_then(|properties| properties.get(key))
            .and_then(Value::as_str)
            .and_then(taxonomy::normalize_version_string)
    };
    capture::client_connected(
        client,
        ClientPlatform {
            os,
            os_version: version("client_os_version"),
            app_version: version("client_app_version"),
        },
    );
}
