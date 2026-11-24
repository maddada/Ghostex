//! Localhost OAuth callback relay for signing a Claude or Codex account into a configured
//! CLIProxyAPI hub (local or remote over Tailscale/LAN).
//!
//! Ghostex on this computer (where the browser runs) briefly binds the provider's hardcoded OAuth
//! callback port on `127.0.0.1` only (`54545` `/callback` for Claude, `1455` `/auth/callback` for
//! Codex), asks the hub's management API for the authorization URL, catches the single redirect,
//! validates `state` in constant time, forwards `{provider, code, state}` to the hub, polls the
//! hub until token exchange completes, answers the browser with a clean dark confirmation page
//! ("Signed in, you can close this tab."), and refreshes the hub's account list.
//!
//! Security invariants:
//! - Loopback only (`127.0.0.1`), never `0.0.0.0`.
//! - Single-use listener: dropped immediately once a valid-state callback arrives, or on cancel /
//!   5-minute timeout / error.
//! - Constant-time state check; requests with missing or mismatched state get HTTP 400 and are
//!   never forwarded.
//! - Management keys, OAuth codes and state tokens are never logged or printed.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::{client, store};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

pub(crate) const CLAUDE_CALLBACK_PORT: u16 = 54545;
const CLAUDE_CALLBACK_PATH: &str = "/callback";
pub(crate) const CODEX_CALLBACK_PORT: u16 = 1455;
const CODEX_CALLBACK_PATH: &str = "/auth/callback";

/// Short timeout for waiting on the browser OAuth callback (5 minutes).
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
/// How long to wait for the hub to finish exchanging the code and saving the account.
const HUB_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(30);
const HUB_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Max HTTP request line size read from the local callback socket.
const MAX_REQUEST_LINE_BYTES: usize = 8192;
/// Finished relays stay briefly so the polling Settings UI can read their outcome.
const FINISHED_RELAY_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, PartialEq, Eq)]
enum HubProvider {
    Claude,
    Codex,
}

impl HubProvider {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    fn port(self) -> u16 {
        match self {
            Self::Claude => CLAUDE_CALLBACK_PORT,
            Self::Codex => CODEX_CALLBACK_PORT,
        }
    }

    fn callback_path(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_CALLBACK_PATH,
            Self::Codex => CODEX_CALLBACK_PATH,
        }
    }

    fn busy_port_message(self) -> String {
        match self {
            Self::Claude => format!(
                "Port {CLAUDE_CALLBACK_PORT} on this computer is already in use (for example by Claude Code or another sign-in). Finish or cancel that sign-in, then try again."
            ),
            Self::Codex => format!(
                "Port {CODEX_CALLBACK_PORT} on this computer is already in use (for example by Codex or another sign-in). Finish or cancel that sign-in, then try again."
            ),
        }
    }
}

#[derive(Clone)]
pub(crate) enum RelayOutcome {
    Waiting,
    Complete,
    Failed(String),
}

struct RelayState {
    hub_id: String,
    provider: HubProvider,
    cancelled: bool,
    listener_dropped: bool,
    outcome: RelayOutcome,
    finished_at: Option<Instant>,
}

type SharedRelay = Arc<(Mutex<RelayState>, Condvar)>;

fn relays() -> &'static Mutex<HashMap<String, SharedRelay>> {
    static RELAYS: OnceLock<Mutex<HashMap<String, SharedRelay>>> = OnceLock::new();
    RELAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn wait_for_listeners_to_drop(wait_for: Vec<SharedRelay>) {
    let deadline = Instant::now() + Duration::from_millis(800);
    for shared in wait_for {
        let (state, wake) = &*shared;
        let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
        while !guard.listener_dropped {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let (next, _) = wake
                .wait_timeout(guard, remaining)
                .unwrap_or_else(|e| e.into_inner());
            guard = next;
        }
    }
}

fn clear_conflicting_relays(provider: HubProvider, hub_id: Option<&str>) {
    let mut wait_for = Vec::new();
    {
        let mut all = relays().lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|_, shared| {
            let (state, wake) = &**shared;
            let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
            if guard
                .finished_at
                .is_some_and(|at| at.elapsed() >= FINISHED_RELAY_TTL)
            {
                return false;
            }
            let same_hub = hub_id.is_some_and(|id| guard.hub_id == id);
            if guard.provider == provider || same_hub {
                guard.cancelled = true;
                wake.notify_all();
                if !guard.listener_dropped {
                    wait_for.push(shared.clone());
                }
                return false;
            }
            true
        });
    }
    wait_for_listeners_to_drop(wait_for);
}

/// Cancels any active local callback listener for `state` and waits briefly for its port to close.
pub(crate) fn cancel_relay(oauth_state: &str) {
    let removed = relays()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(oauth_state);
    if let Some(shared) = removed {
        let (state, wake) = &*shared;
        let needs_wait = {
            let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
            guard.cancelled = true;
            wake.notify_all();
            !guard.listener_dropped
        };
        if needs_wait {
            wait_for_listeners_to_drop(vec![shared]);
        }
    }
}

/// Reads the current outcome of a local relay for `oauth_state`, removing finished entries once
/// reported so `hubOAuthStatus` receives terminal local errors (like timeout or provider error)
/// cleanly.
pub(crate) fn relay_outcome(oauth_state: &str) -> Option<RelayOutcome> {
    let mut all = relays().lock().unwrap_or_else(|e| e.into_inner());
    let outcome = all.get(oauth_state).map(|shared| {
        shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .outcome
            .clone()
    })?;
    if !matches!(outcome, RelayOutcome::Waiting) {
        all.remove(oauth_state);
    }
    Some(outcome)
}

/// Binds `127.0.0.1:54545` (Claude) or `127.0.0.1:1455` (Codex) on this computer, requests the
/// OAuth URL from `hub`, and spawns the single-use 5-minute callback relay thread.
pub(crate) fn start_local_relay(
    paths: &GxserverPaths,
    hub: &store::Hub,
    management_key: &str,
    provider_text: &str,
) -> Result<client::OAuthStart, DomainStateError> {
    let provider = HubProvider::parse(provider_text).ok_or_else(|| {
        DomainStateError::bad_request(
            "Only Claude and Codex accounts can be added to a CLIProxyAPI hub.",
        )
    })?;

    // Release any prior Ghostex relay holding this provider's port before binding; leave other
    // providers' relays untouched until binding succeeds.
    clear_conflicting_relays(provider, None);

    let listener = TcpListener::bind(("127.0.0.1", provider.port())).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            DomainStateError::bad_request(provider.busy_port_message())
        } else {
            DomainStateError::bad_request(format!(
                "Couldn't listen on 127.0.0.1:{} for the sign-in callback ({error}).",
                provider.port()
            ))
        }
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        DomainStateError::bad_request(format!(
            "Couldn't prepare the sign-in listener on 127.0.0.1:{} ({error}).",
            provider.port()
        ))
    })?;

    clear_conflicting_relays(provider, Some(&hub.id));

    let started = match client::oauth_start(&hub.url, management_key, provider.id(), true) {
        Ok(started) => started,
        Err(error) => {
            drop(listener);
            return Err(DomainStateError::bad_request(error));
        }
    };

    let shared: SharedRelay = Arc::new((
        Mutex::new(RelayState {
            hub_id: hub.id.clone(),
            provider,
            cancelled: false,
            listener_dropped: false,
            outcome: RelayOutcome::Waiting,
            finished_at: None,
        }),
        Condvar::new(),
    ));
    relays()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(started.state.clone(), shared.clone());

    let paths = paths.clone();
    let hub = hub.clone();
    let key = management_key.to_string();
    let oauth_state = started.state.clone();
    std::thread::spawn(move || {
        let outcome = run_relay(
            &shared,
            listener,
            &paths,
            &hub,
            &key,
            provider,
            &oauth_state,
        );
        let (state, wake) = &*shared;
        let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard.listener_dropped = true;
        guard.finished_at = Some(Instant::now());
        guard.outcome = match outcome {
            Ok(()) => RelayOutcome::Complete,
            Err(message) => RelayOutcome::Failed(message),
        };
        wake.notify_all();
    });

    Ok(started)
}

fn wait_or_cancelled(shared: &SharedRelay, duration: Duration) -> bool {
    let (state, wake) = &**shared;
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());
    if guard.cancelled {
        return true;
    }
    let (guard, _) = wake
        .wait_timeout(guard, duration)
        .unwrap_or_else(|e| e.into_inner());
    guard.cancelled
}

fn mark_listener_dropped(shared: &SharedRelay) {
    let (state, wake) = &**shared;
    let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
    guard.listener_dropped = true;
    wake.notify_all();
}

fn run_relay(
    shared: &SharedRelay,
    listener: TcpListener,
    paths: &GxserverPaths,
    hub: &store::Hub,
    management_key: &str,
    provider: HubProvider,
    expected_state: &str,
) -> Result<(), String> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    let (stream, callback) = loop {
        if wait_or_cancelled(shared, Duration::from_millis(150)) {
            drop(listener);
            mark_listener_dropped(shared);
            return Err("Sign-in cancelled.".to_string());
        }
        if Instant::now() >= deadline {
            drop(listener);
            mark_listener_dropped(shared);
            let _ = client::oauth_cancel(&hub.url, management_key, expected_state);
            return Err("The sign-in timed out after 5 minutes. Try again.".to_string());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some((stream, outcome)) =
                    handle_connection(stream, provider.callback_path(), expected_state)
                {
                    break (stream, outcome);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                drop(listener);
                mark_listener_dropped(shared);
                let _ = client::oauth_cancel(&hub.url, management_key, expected_state);
                return Err(format!("The local sign-in listener stopped ({error})."));
            }
        }
    };

    // Single-use listener: release the loopback port immediately once the matching callback arrives.
    drop(listener);
    mark_listener_dropped(shared);

    let code = match callback {
        CallbackOutcome::Code(code) => code,
        CallbackOutcome::ProviderError(message) => {
            let _ = client::oauth_cancel(&hub.url, management_key, expected_state);
            write_html_response(
                stream,
                400,
                "Bad Request",
                false,
                "Sign-in didn't finish",
                &message,
            );
            return Err(message);
        }
    };

    if let Err(error) = client::oauth_callback_code(
        &hub.url,
        management_key,
        provider.id(),
        expected_state,
        &code,
    ) {
        write_html_response(
            stream,
            502,
            "Bad Gateway",
            false,
            "Couldn't reach the hub",
            &error,
        );
        return Err(error);
    }

    let poll_deadline = Instant::now() + HUB_EXCHANGE_TIMEOUT;
    loop {
        if wait_or_cancelled(shared, Duration::ZERO) {
            write_html_response(
                stream,
                200,
                "OK",
                false,
                "Sign-in cancelled",
                "You cancelled this sign-in in Ghostex.",
            );
            return Err("Sign-in cancelled.".to_string());
        }
        match client::oauth_status(&hub.url, management_key, expected_state) {
            Ok(status) if status.status == "ok" => {
                write_html_response(
                    stream,
                    200,
                    "OK",
                    true,
                    "Signed in",
                    "Signed in, you can close this tab.",
                );
                super::refresh_after_sign_in(paths, hub);
                return Ok(());
            }
            Ok(status) if status.status == "error" => {
                let message = status.error.unwrap_or_else(|| {
                    "The hub couldn't finish signing the account in.".to_string()
                });
                write_html_response(
                    stream,
                    400,
                    "Bad Request",
                    false,
                    "Sign-in didn't finish",
                    &message,
                );
                return Err(message);
            }
            Ok(_) => {}
            Err(error) => {
                write_html_response(
                    stream,
                    502,
                    "Bad Gateway",
                    false,
                    "Couldn't confirm sign-in",
                    &error,
                );
                return Err(error);
            }
        }
        if Instant::now() >= poll_deadline {
            let message = "The hub took too long to finish the sign-in. Refresh the hub to check if the account appeared.".to_string();
            write_html_response(
                stream,
                504,
                "Gateway Timeout",
                false,
                "Still waiting on the hub",
                &message,
            );
            return Err(message);
        }
        if wait_or_cancelled(shared, HUB_POLL_INTERVAL) {
            write_html_response(
                stream,
                200,
                "OK",
                false,
                "Sign-in cancelled",
                "You cancelled this sign-in in Ghostex.",
            );
            return Err("Sign-in cancelled.".to_string());
        }
    }
}

enum CallbackOutcome {
    Code(String),
    ProviderError(String),
}

/// Constant-time string comparison for OAuth `state` validation.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Inspects one loopback HTTP request. Returns `Some((stream, outcome))` only when the request
/// targets `callback_path` with a matching `state`; otherwise answers the socket (404/400) and
/// returns `None` so the listener keeps waiting for the real callback.
fn handle_connection(
    stream: TcpStream,
    callback_path: &str,
    expected_state: &str,
) -> Option<(TcpStream, CallbackOutcome)> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let mut line = String::new();
    let read_ok = BufReader::new((&stream).take(MAX_REQUEST_LINE_BYTES as u64))
        .read_line(&mut line)
        .is_ok();
    if !read_ok || line.is_empty() {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let Ok(parsed) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        write_html_response(
            stream,
            400,
            "Bad Request",
            false,
            "Invalid request",
            "Go back to Ghostex and try again.",
        );
        return None;
    };
    if method != "GET" || parsed.path() != callback_path {
        write_plain_404(stream);
        return None;
    }

    let pairs: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    let got_state = pairs.get("state").map(String::as_str).unwrap_or_default();
    if got_state.is_empty() || !constant_time_eq(got_state, expected_state) {
        write_html_response(
            stream,
            400,
            "Bad Request",
            false,
            "Sign-in state didn't match",
            "This sign-in link doesn't match the active login. Go back to Ghostex and try again.",
        );
        return None;
    }

    if let Some(err) = pairs
        .get("error_description")
        .or_else(|| pairs.get("error"))
        .map(|text| client::masked(text))
        .filter(|text| !text.is_empty())
    {
        return Some((
            stream,
            CallbackOutcome::ProviderError(format!("Sign-in didn't finish: {err}")),
        ));
    }
    let code = pairs
        .get("code")
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    match code {
        Some(code) => Some((stream, CallbackOutcome::Code(code))),
        None => Some((
            stream,
            CallbackOutcome::ProviderError(
                "The sign-in callback had no authorization code. Go back to Ghostex and try again."
                    .to_string(),
            ),
        )),
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn write_plain_404(mut stream: TcpStream) {
    let body = "Not found";
    let _ = write!(
        stream,
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

fn write_html_response(
    mut stream: TcpStream,
    status: u16,
    reason: &str,
    ok: bool,
    heading: &str,
    detail: &str,
) {
    let heading_html = escape_html(heading);
    let detail_html = escape_html(detail);
    let accent = if ok { "#34d399" } else { "#f87171" };
    let badge_bg = if ok {
        "rgba(52,211,153,0.12)"
    } else {
        "rgba(248,113,113,0.12)"
    };
    let badge_border = if ok {
        "rgba(52,211,153,0.35)"
    } else {
        "rgba(248,113,113,0.35)"
    };
    let icon = if ok {
        r#"<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"#
    } else {
        r#"<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 8v4"/><path d="M12 16h.01"/></svg>"#
    };
    let sub = if ok {
        "<p class=\"sub\">Ghostex is refreshing your hub's account list in Settings.</p>"
    } else {
        "<p class=\"sub\">Return to Settings &gt; Accounts in Ghostex to try again.</p>"
    };
    let body = format!(
        "<!doctype html>\
<html lang=\"en\">\
<head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<meta name=\"color-scheme\" content=\"dark\">\
<title>{heading_html} - Ghostex</title>\
<style>\
*{{box-sizing:border-box}}\
html,body{{height:100%;margin:0;background:#0e0f12;color:#f3f4f6;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Inter,Roboto,sans-serif;-webkit-font-smoothing:antialiased}}\
body{{display:flex;align-items:center;justify-content:center;padding:24px}}\
.card{{width:100%;max-width:440px;padding:32px 28px;border-radius:14px;background:#16181d;border:1px solid rgba(255,255,255,0.09);box-shadow:0 20px 50px rgba(0,0,0,0.45);text-align:center}}\
.badge{{width:48px;height:48px;margin:0 auto 18px;border-radius:12px;display:flex;align-items:center;justify-content:center;color:{accent};background:{badge_bg};border:1px solid {badge_border}}}\
h1{{margin:0 0 10px;font-size:20px;font-weight:600;letter-spacing:-0.01em;color:#f9fafb}}\
p{{margin:0;font-size:14.5px;line-height:1.55;color:#d1d5db}}\
.sub{{margin-top:12px;font-size:13px;color:#9ca3af}}\
.brand{{margin-top:22px;padding-top:16px;border-top:1px solid rgba(255,255,255,0.07);font-size:12px;color:#6b7280;letter-spacing:0.02em}}\
</style>\
</head>\
<body>\
<main class=\"card\">\
<div class=\"badge\">{icon}</div>\
<h1>{heading_html}</h1>\
<p>{detail_html}</p>\
{sub}\
<div class=\"brand\">Ghostex · CLIProxyAPI hub</div>\
</main>\
</body>\
</html>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}
