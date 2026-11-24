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
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use crate::app::helpers::*;
use crate::*;

const GPUI_CLIENT_EVENT_PATH: &str = "/api/recordClientEvent";
const GPUI_CLIENT_EVENT_TIMEOUT: Duration = Duration::from_secs(5);
const GPUI_CLIENT_EVENT_RETRY_BUDGET: Duration = Duration::from_secs(10);
const GPUI_CLIENT_EVENT_MAX_ATTEMPTS: usize = 4;
const GPUI_CLIENT_EVENT_MAX_RESPONSE_BYTES: usize = 4_096;
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
/// that had to respawn the daemon still reports exactly one event. Only latches
/// permanently once gxserver acknowledges with 2xx, so a transient Windows
/// startup/crowded-connect failure does not drop the launch ping for good.
pub(crate) fn record_gpui_app_launched_telemetry(background: &gpui::BackgroundExecutor) {
    const UNSENT: u8 = 0;
    const SENDING: u8 = 1;
    const SENT: u8 = 2;
    static STATE: AtomicU8 = AtomicU8::new(UNSENT);
    if STATE
        .compare_exchange(UNSENT, SENDING, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let mut properties = serde_json::json!({
        "client": "desktop",
        "app_version": GPUI_APP_MARKETING_VERSION,
    });
    /*
    CDXC:Telemetry 2026-11-20:
    On Linux, the desktop environment (or compositor) and session type this app runs in, as the
    environment names them. gxserver maps both onto closed tables and drops anything else
    (server/src/telemetry/settings_snapshot.rs), and a gxserver started by systemd may not have
    these variables itself, which is why the app sends them.
    */
    if cfg!(target_os = "linux") {
        let env = |name: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        };
        if let Some(desktop) = env("XDG_CURRENT_DESKTOP").or_else(|| env("XDG_SESSION_DESKTOP")) {
            properties["linux_desktop"] = serde_json::Value::String(desktop);
        }
        let session = env("XDG_SESSION_TYPE")
            .or_else(|| env("WAYLAND_DISPLAY").map(|_| "wayland".to_string()));
        if let Some(session) = session {
            properties["linux_session"] = serde_json::Value::String(session);
        }
    }
    let body = build_gpui_client_event_body("app.launched", properties);
    background
        .spawn(async move {
            let next = if post_gpui_client_event_body(&body) {
                SENT
            } else {
                UNSENT
            };
            STATE.store(next, Ordering::SeqCst);
        })
        .detach();
}

/// `surface.opened`, throttled to at most one ping per surface (and per
/// extension id) per hour so a user flipping between views produces usage
/// signal, not traffic; gxserver applies the same window. The map is in-memory
/// only and resets with the process. A failed startup delivery clears its
/// reservation so the next surface open is not silenced for an hour.
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
    let body = build_gpui_client_event_body("surface.opened", surface.properties());
    background
        .spawn(async move {
            if !post_gpui_client_event_body(&body) {
                if let Ok(mut last_sent) =
                    LAST_SENT.get_or_init(|| Mutex::new(HashMap::new())).lock()
                {
                    if last_sent.get(&surface) == Some(&now) {
                        last_sent.remove(&surface);
                    }
                }
            }
        })
        .detach();
}

/// The sidebar's layout changed: `layout` is "rail" or "classic", `source` "settings" or "intro"
/// (gxserver re-checks both, server/src/telemetry/taxonomy.rs `SIDEBAR_LAYOUTS`).
pub(crate) fn record_gpui_sidebar_layout_changed_telemetry(
    layout: &'static str,
    source: &'static str,
) {
    record_gpui_client_event_on_thread(
        "sidebar_layout.changed",
        serde_json::json!({ "layout": layout, "source": source }),
    );
}

/// Meet the rail opened, on `layout` ("rail" or "classic").
pub(crate) fn record_gpui_rail_intro_shown_telemetry(layout: &'static str) {
    record_gpui_client_event_on_thread("rail_intro.shown", serde_json::json!({ "layout": layout }));
}

/// Meet the rail closed (Done, Escape or its corner) keeping `layout`.
pub(crate) fn record_gpui_rail_intro_answered_telemetry(layout: &'static str) {
    record_gpui_client_event_on_thread(
        "rail_intro.answered",
        serde_json::json!({ "chose": layout }),
    );
}

/// Zero-PII onboarding milestone (`server/src/telemetry/taxonomy.rs` `ONBOARDING_STEPS`).
#[allow(dead_code)]
pub(crate) fn record_gpui_onboarding_step_telemetry(step: &'static str) {
    record_gpui_client_event_on_thread("onboarding.step", serde_json::json!({ "step": step }));
}

/// Zero-PII reliability error category (`server/src/telemetry/taxonomy.rs`
/// `RELIABILITY_ERROR_CATEGORIES`).
#[allow(dead_code)]
pub(crate) fn record_gpui_reliability_error_telemetry(
    category: &'static str,
    agent_id: Option<&'static str>,
) {
    let mut properties = serde_json::json!({ "category": category });
    if let Some(agent_id) = agent_id {
        properties["agent_id"] = serde_json::Value::String(agent_id.to_string());
    }
    record_gpui_client_event_on_thread("reliability.error", properties);
}

/// `record_gpui_client_event` from where no executor is at hand (a settings read): the same body,
/// posted from a short-lived thread.
fn record_gpui_client_event_on_thread(event: &'static str, properties: serde_json::Value) {
    let body = build_gpui_client_event_body(event, properties);
    let _ = std::thread::Builder::new()
        .name("ghostex-analytics-ping".to_string())
        .spawn(move || {
            let _ = post_gpui_client_event_body(&body);
        });
}

/// Started with the analytics launch option: `GHOSTEX_TELEMETRY_DISABLED=1`, `DO_NOT_TRACK=1`, or
/// `--no-analytics` (which `main` turns into the first). Read once; the environment cannot change.
fn gpui_analytics_off_at_launch() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| {
        ["GHOSTEX_TELEMETRY_DISABLED", "DO_NOT_TRACK"]
            .iter()
            .any(|name| {
                std::env::var(name).is_ok_and(|value| {
                    let value = value.trim();
                    value == "1" || value.eq_ignore_ascii_case("true")
                })
            })
    })
}

fn build_gpui_client_event_body(event: &'static str, properties: serde_json::Value) -> String {
    /*
    With the launch option on, every ping becomes `analytics.off` instead: a gxserver that was
    already running (or that a later start brought up without the variable) is opted out until it
    exits, and nothing about the event is sent (server/src/telemetry/gate.rs).
    */
    if gpui_analytics_off_at_launch() {
        serde_json::json!({ "event": "analytics.off" })
    } else {
        serde_json::json!({
            "event": event,
            "properties": properties,
        })
    }
    .to_string()
}

/// Raw-`TcpStream` loopback POST, mirroring the transport shape of
/// `gxserver_post_typed_operation` (localhost only, bearer token from the
/// gxserver state dir, protocol header, bounded timeouts).
///
/// Always reads the HTTP response before closing the socket: dropping the
/// stream immediately after `write_all` causes Windows Winsock to issue a TCP
/// RST while hyper/axum is still reading or routing the request, dropping ~75%
/// of Windows desktop pings. Also waits out bounded transient startup races
/// (503 `serverStarting`, crowded loopback listen queue, or auth token not yet
/// written) while failing closed immediately if another account owns the port.
fn post_gpui_client_event_body(body: &str) -> bool {
    let started = Instant::now();
    let deadline = started + GPUI_CLIENT_EVENT_RETRY_BUDGET;
    for attempt in 0..GPUI_CLIENT_EVENT_MAX_ATTEMPTS {
        let port = gpui_local_gxserver_api_port();
        let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{port}");
        let Ok(socket_address) = address.parse::<SocketAddr>() else {
            return false;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let timeout = remaining.min(GPUI_CLIENT_EVENT_TIMEOUT);
        let mut stream = match TcpStream::connect_timeout(&socket_address, timeout) {
            Ok(stream) => stream,
            Err(error) => {
                if !is_retryable_loopback_error(&error) {
                    return false;
                }
                if !sleep_before_retry(attempt, deadline, None) {
                    return false;
                }
                continue;
            }
        };
        // Check loopback listener ownership after connect and before sending the token
        // (matching `typed_operations.rs`). A foreign listener always fails closed with no retry.
        if ghostex_paths::loopback_owner::check_host_port(
            &socket_address.ip().to_string(),
            socket_address.port(),
        )
        .is_err()
        {
            return false;
        }
        let Ok(token) = read_gpui_gxserver_auth_token() else {
            if !sleep_before_retry(attempt, deadline, None) {
                return false;
            }
            continue;
        };
        let io_timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(GPUI_CLIENT_EVENT_TIMEOUT)
            .max(Duration::from_millis(250));
        if stream.set_write_timeout(Some(io_timeout)).is_err()
            || stream.set_read_timeout(Some(io_timeout)).is_err()
        {
            return false;
        }
        let request = format!(
            "POST {GPUI_CLIENT_EVENT_PATH} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        if stream.write_all(request.as_bytes()).is_err() || stream.flush().is_err() {
            if !sleep_before_retry(attempt, deadline, None) {
                return false;
            }
            continue;
        }
        let mut response = Vec::with_capacity(256);
        let _ = stream
            .take(GPUI_CLIENT_EVENT_MAX_RESPONSE_BYTES as u64)
            .read_to_end(&mut response);
        let Some((status, response_body)) = parse_client_event_http_response(&response) else {
            if !sleep_before_retry(attempt, deadline, None) {
                return false;
            }
            continue;
        };
        if (200..300).contains(&status) {
            return true;
        }
        if status == 503 {
            let delay = ghostex_gx_protocol::server_starting_retry_after(status, response_body);
            if !sleep_before_retry(attempt, deadline, delay) {
                return false;
            }
            continue;
        }
        return false;
    }
    false
}

fn is_retryable_loopback_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::AddrNotAvailable
            | ErrorKind::TimedOut
            | ErrorKind::WouldBlock
    )
}

fn sleep_before_retry(attempt: usize, deadline: Instant, advised: Option<Duration>) -> bool {
    if attempt + 1 >= GPUI_CLIENT_EVENT_MAX_ATTEMPTS {
        return false;
    }
    let delay = advised.unwrap_or_else(|| Duration::from_millis(200 * (attempt as u64 + 1)));
    let now = Instant::now();
    if now + delay >= deadline {
        return false;
    }
    std::thread::sleep(delay);
    true
}

fn parse_client_event_http_response(response: &[u8]) -> Option<(u16, &str)> {
    let split = response.windows(4).position(|window| window == b"\r\n\r\n")?;
    let headers = std::str::from_utf8(&response[..split]).ok()?;
    let status = headers
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()?;
    let body = std::str::from_utf8(&response[split + 4..]).unwrap_or("");
    Some((status, body))
}
