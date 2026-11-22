//! The answer gxserver gives between taking its port and being ready to serve.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request, Response, StatusCode},
    middleware::Next,
};
use serde_json::json;

use crate::constants::{GXSERVER_PRODUCT, GXSERVER_PROTOCOL_VERSION};
use ghostex_gx_protocol::{SERVER_STARTING_ERROR, SERVER_STARTING_RETRY_AFTER_MS};

use super::{apply_cors_headers, json_response, AppState};

/// CDXC:ServerDaemon 2026-10-22 DECISION:
/// User: "Yes, do both": gxserver answers "starting" while it holds its port but is not ready, and a startup that hangs is replaced. gxserver used to take its port and then spend seconds to minutes in startup work (the zmx/wmx wire check above all, whose wmx.exe starts antivirus can slow without limit) before serving, so clients connected and got no answer ("Could not read gxserver response") and a hung startup was never replaced. The port is now served from the moment it is taken; until startup finishes every request except a CORS preflight gets HTTP 503 `{ok:false, error:"serverStarting", state:"starting", step, startingForMs, retryAfterMs}`, and nothing behind it runs, so a client may resend the request; every client that used to wait in the listen backlog does so (packages/gx-protocol/src/starting.rs). Older clients and older gxservers already read a non-2xx health answer as "no daemon answering yet", so mixed versions during an update behave as before, only faster.
/// SEE-ALSO: `bind_local_listener_taking_over_stale_holder` in stale_server_takeover.rs (the hang replacement), `fetch_server_starting` in server/src/http_client.rs and `get_gxserver_status` in server/src/cli.rs (the CLI's "starting" state), `gpui_probe_local_gxserver_health_with_diagnostics` in apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs (the app's Starting state).
pub(crate) struct StartupGate {
    ready: AtomicBool,
    started: Instant,
    step: Mutex<&'static str>,
}

impl StartupGate {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            ready: AtomicBool::new(false),
            started: Instant::now(),
            step: Mutex::new("taking its port"),
        })
    }

    /// Names the startup step now running, in words a client can show.
    pub(crate) fn step(&self, step: &'static str) {
        *self.step.lock().unwrap_or_else(|error| error.into_inner()) = step;
    }

    /// Lets every request through from now on.
    pub(crate) fn open(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    fn starting_response(&self) -> Response<Body> {
        let step = *self.step.lock().unwrap_or_else(|error| error.into_inner());
        let starting_for = self.started.elapsed();
        let mut response = json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "error": SERVER_STARTING_ERROR,
                "message": format!(
                    "gxserver is still starting ({step}, {} s so far). Try again in a moment.",
                    starting_for.as_secs()
                ),
                "ok": false,
                "product": GXSERVER_PRODUCT,
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                "retryAfterMs": SERVER_STARTING_RETRY_AFTER_MS,
                "startingForMs": starting_for.as_millis() as u64,
                "state": "starting",
                "step": step,
            }),
        );
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        response
    }
}

/// The middleware in front of every route: requests pass once the gate is open.
pub(crate) async fn startup_gate_middleware(
    gate: Arc<StartupGate>,
    state: Arc<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    if gate.ready.load(Ordering::SeqCst) || request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    let mut response = gate.starting_response();
    apply_cors_headers(request.headers(), &mut response, &state.config);
    response
}
