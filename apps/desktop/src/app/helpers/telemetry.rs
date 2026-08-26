// Anonymous-analytics loopback pings (docs/2026-08-26/anonymous-analytics/PLAN.md).
//
// The desktop app never talks to PostHog. It POSTs a tiny fixed-vocabulary
// event to the local gxserver, which owns the `analyticsEnabled` opt-out gate,
// the property whitelist, and all outbound egress. That single chokepoint is
// why nothing here consults the setting: the daemon drops events when
// analytics is off.
//
// Every ping is strictly fire-and-forget on the background executor: short
// timeouts, all errors swallowed, no retries, no queue, no persistence, and no
// call site may observe the result. Only the closed enum strings defined in the
// spec may ever be sent; anything unmappable sends nothing at all.

use std::{
    collections::HashMap,
    io::Write,
    net::{SocketAddr, TcpStream},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::app::helpers::*;
use crate::*;

const GPUI_CLIENT_EVENT_PATH: &str = "/api/recordClientEvent";
const GPUI_CLIENT_EVENT_TIMEOUT: Duration = Duration::from_secs(2);
const GPUI_SURFACE_OPENED_THROTTLE: Duration = Duration::from_secs(60 * 60);

/// Shipping marketing version baked in by `apps/desktop/build.rs`; dev builds
/// have no `GHOSTEX_GPUI_MARKETING_VERSION` in the environment and report the
/// crate version instead. Same pattern as `bin/ghostex_windows_installer.rs`.
pub(crate) const GPUI_APP_MARKETING_VERSION: &str =
    match option_env!("GHOSTEX_BUILD_MARKETING_VERSION") {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    };

/// The complete surface vocabulary the desktop app may report. Anything not in
/// this list is not sent — internal mode ids, extension ids, and modal ids are
/// never forwarded.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GpuiTelemetrySurface {
    Agents,
    Code,
    Browser,
    Kanban,
    Automate,
    Docs,
    Find,
    ExtensionsStore,
    Settings,
}

impl GpuiTelemetrySurface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Code => "code",
            Self::Browser => "browser",
            Self::Kanban => "kanban",
            Self::Automate => "automate",
            Self::Docs => "docs",
            Self::Find => "find",
            Self::ExtensionsStore => "extensions_store",
            Self::Settings => "settings",
        }
    }
}

/// Titlebar workarea → spec surface. `Extension(_)` deliberately maps to
/// `None`: extension ids are author-supplied text and must never leave the
/// machine.
pub(crate) fn gpui_telemetry_surface_for_titlebar_mode(
    mode: TitlebarMode,
) -> Option<GpuiTelemetrySurface> {
    match mode {
        TitlebarMode::Agents => Some(GpuiTelemetrySurface::Agents),
        TitlebarMode::Source => Some(GpuiTelemetrySurface::Code),
        TitlebarMode::Browser => Some(GpuiTelemetrySurface::Browser),
        TitlebarMode::Kanban => Some(GpuiTelemetrySurface::Kanban),
        TitlebarMode::Automate => Some(GpuiTelemetrySurface::Automate),
        TitlebarMode::Manage => Some(GpuiTelemetrySurface::Docs),
        TitlebarMode::Extension(_) => None,
    }
}

/// App modal → spec surface. Only the three modals the spec names are
/// reportable; every other modal (including per-extension modals) sends
/// nothing.
pub(crate) fn gpui_telemetry_surface_for_app_modal(
    modal: GpuiAppModalKind,
) -> Option<GpuiTelemetrySurface> {
    match modal {
        GpuiAppModalKind::FindPrompts => Some(GpuiTelemetrySurface::Find),
        GpuiAppModalKind::ExtensionsBrowser => Some(GpuiTelemetrySurface::ExtensionsStore),
        GpuiAppModalKind::Settings => Some(GpuiTelemetrySurface::Settings),
        _ => None,
    }
}

/// `app.launched`, at most once per app process. Called from every branch of
/// the gxserver bootstrap that confirms a healthy local daemon, so a launch
/// that had to respawn the daemon still reports exactly one event.
pub(crate) fn record_gpui_app_launched_telemetry(background: &gpui::BackgroundExecutor) {
    static SENT: AtomicBool = AtomicBool::new(false);
    if SENT.swap(true, Ordering::SeqCst) {
        return;
    }
    record_gpui_client_event(
        "app.launched",
        serde_json::json!({
            "client": "desktop",
            "app_version": GPUI_APP_MARKETING_VERSION,
        }),
        background,
    );
}

/// `surface.opened`, throttled to at most one ping per surface per hour so a
/// user flipping between workareas produces usage signal, not traffic. The map
/// is in-memory only and resets with the process.
pub(crate) fn record_gpui_surface_opened_telemetry(
    surface: GpuiTelemetrySurface,
    background: &gpui::BackgroundExecutor,
) {
    static LAST_SENT: OnceLock<Mutex<HashMap<GpuiTelemetrySurface, Instant>>> = OnceLock::new();
    let now = Instant::now();
    let throttled = {
        let Ok(mut last_sent) = LAST_SENT.get_or_init(|| Mutex::new(HashMap::new())).lock() else {
            return;
        };
        match last_sent.get(&surface) {
            Some(sent_at) if now.duration_since(*sent_at) < GPUI_SURFACE_OPENED_THROTTLE => true,
            _ => {
                last_sent.insert(surface, now);
                false
            }
        }
    };
    if throttled {
        return;
    }
    record_gpui_client_event(
        "surface.opened",
        serde_json::json!({ "surface": surface.as_str() }),
        background,
    );
}

fn record_gpui_client_event(
    event: &'static str,
    properties: serde_json::Value,
    background: &gpui::BackgroundExecutor,
) {
    let body = serde_json::json!({
        "event": event,
        "properties": properties,
    })
    .to_string();
    background
        .spawn(async move {
            post_gpui_client_event_body(&body);
        })
        .detach();
}

/// One raw-`TcpStream` loopback POST, mirroring the transport shape of
/// `gxserver_post_typed_operation` (localhost only, bearer token from the
/// gxserver state dir, protocol header, short timeouts). The response is not
/// read: the endpoint always answers 2xx and drops anything it dislikes, so
/// there is nothing for the client to inspect or retry.
fn post_gpui_client_event_body(body: &str) {
    let Ok(token) = read_gpui_gxserver_auth_token() else {
        return;
    };
    let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}");
    let Ok(socket_address) = address.parse::<SocketAddr>() else {
        return;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&socket_address, GPUI_CLIENT_EVENT_TIMEOUT)
    else {
        return;
    };
    if stream
        .set_write_timeout(Some(GPUI_CLIENT_EVENT_TIMEOUT))
        .is_err()
    {
        return;
    }
    let request = format!(
        "POST {GPUI_CLIENT_EVENT_PATH} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(request.as_bytes());
    let _ = stream.flush();
}
