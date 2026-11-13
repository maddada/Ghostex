// Anonymous-analytics loopback pings (docs/2026-08-26/anonymous-analytics/PLAN.md).
//
// The desktop app never talks to PostHog. It POSTs a tiny fixed-vocabulary
// event to the local gxserver, which owns the analytics opt-out gate,
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

/// The app version baked in by `apps/desktop/build.rs` (the release version, or the root
/// `package.json` version for dev builds), never the placeholder crate version.
pub(crate) const GPUI_APP_MARKETING_VERSION: &str = env!("GHOSTEX_BUILD_MARKETING_VERSION");

/// The complete surface vocabulary the desktop app may report. Anything not in
/// this list is not sent.
///
/// CDXC:Telemetry 2026-10-27:
/// User: "which views being accessesd the most". Every view the side panel can
/// show now reports, including Changes, Terminal, the Bot automations feed, the
/// Work page, Claude and extension views. An extension view sends its id only
/// over loopback; gxserver turns it into a built-in or official Store id, or
/// `custom`, before anything is queued (`server/src/telemetry/extensions.rs`),
/// so a user's own extension or view name never leaves the machine.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GpuiTelemetrySurface {
    Agents,
    Code,
    Browser,
    Kanban,
    Automate,
    Docs,
    Changes,
    Terminal,
    Automations,
    Work,
    Claude,
    Extension(ExtensionId),
    Find,
    Settings,
    QuickAccess,
    PreviousSessions,
    AgentsHub,
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
            Self::Changes => "changes",
            Self::Terminal => "terminal",
            Self::Automations => "automations",
            Self::Work => "work",
            Self::Claude => "claude",
            Self::Extension(_) => "extension",
            Self::Find => "find",
            Self::Settings => "settings",
            Self::QuickAccess => "quick_access",
            Self::PreviousSessions => "previous_sessions",
            Self::AgentsHub => "agents_hub",
        }
    }

    /// The `surface.opened` properties this ping sends to gxserver.
    fn properties(self) -> serde_json::Value {
        let mut properties = serde_json::json!({
            "surface": self.as_str(),
            "client": "desktop",
        });
        if let Self::Extension(id) = self {
            properties["extension"] = serde_json::Value::String(id.as_str().to_string());
        }
        properties
    }
}

/// Side-panel view → spec surface. The built-in Claude view is its own surface;
/// every other extension view (website views such as GitHub, Storybook, Store
/// and local extensions, custom views) reports as `extension`.
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
        TitlebarMode::Changes => Some(GpuiTelemetrySurface::Changes),
        TitlebarMode::Terminal => Some(GpuiTelemetrySurface::Terminal),
        TitlebarMode::BotFeed => Some(GpuiTelemetrySurface::Automations),
        TitlebarMode::Work => Some(GpuiTelemetrySurface::Work),
        // Not a surface of the telemetry spec: a side chat is a chat, not a view.
        TitlebarMode::SideChat => None,
        mode if mode.is_claude_web() => Some(GpuiTelemetrySurface::Claude),
        TitlebarMode::Extension(id) => Some(GpuiTelemetrySurface::Extension(id)),
    }
}

/// App modal → spec surface. The extensions store lost its own surface when it
/// became a Settings tab, so it reports as `settings` like every other Settings
/// page. An extension's own modal reports like its view.
pub(crate) fn gpui_telemetry_surface_for_app_modal(
    modal: GpuiAppModalKind,
) -> Option<GpuiTelemetrySurface> {
    match modal {
        GpuiAppModalKind::FindPrompts => Some(GpuiTelemetrySurface::Find),
        GpuiAppModalKind::Settings => Some(GpuiTelemetrySurface::Settings),
        GpuiAppModalKind::CommandPalette | GpuiAppModalKind::QuickAccessFiles => {
            Some(GpuiTelemetrySurface::QuickAccess)
        }
        GpuiAppModalKind::PreviousSessions => Some(GpuiTelemetrySurface::PreviousSessions),
        GpuiAppModalKind::AgentsHub => Some(GpuiTelemetrySurface::AgentsHub),
        GpuiAppModalKind::Extension(id) => Some(GpuiTelemetrySurface::Extension(id)),
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

/// `surface.opened`, throttled to at most one ping per surface (and per
/// extension id) per hour so a user flipping between views produces usage
/// signal, not traffic; gxserver applies the same window. The map is in-memory
/// only and resets with the process.
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
    record_gpui_client_event("surface.opened", surface.properties(), background);
}

/// Started with the analytics launch option: `GHOSTEX_TELEMETRY_DISABLED=1`, `DO_NOT_TRACK=1`, or
/// `--no-analytics` (which `main` turns into the first). Read once; the environment cannot change.
fn gpui_analytics_off_at_launch() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| {
        ["GHOSTEX_TELEMETRY_DISABLED", "DO_NOT_TRACK"].iter().any(|name| {
            std::env::var(name).is_ok_and(|value| {
                let value = value.trim();
                value == "1" || value.eq_ignore_ascii_case("true")
            })
        })
    })
}

fn record_gpui_client_event(
    event: &'static str,
    properties: serde_json::Value,
    background: &gpui::BackgroundExecutor,
) {
    /*
    With the launch option on, every ping becomes `analytics.off` instead: a gxserver that was
    already running (or that a later start brought up without the variable) is opted out until it
    exits, and nothing about the event is sent (server/src/telemetry/gate.rs).
    */
    let body = if gpui_analytics_off_at_launch() {
        serde_json::json!({ "event": "analytics.off" })
    } else {
        serde_json::json!({
            "event": event,
            "properties": properties,
        })
    }
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
    let port = gpui_local_gxserver_api_port();
    let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{port}");
    let Ok(socket_address) = address.parse::<SocketAddr>() else {
        return;
    };
    if ghostex_paths::loopback_owner::check_host_port(
        &socket_address.ip().to_string(),
        socket_address.port(),
    )
    .is_err()
    {
        return;
    }
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
