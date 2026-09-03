/*
CDXC:Telemetry 2026-08-26:
Anonymous PostHog analytics. Spec: `docs/2026-08-26/anonymous-analytics/PLAN.md`.

ALL analytics egress in Ghostex happens here, in gxserver. The desktop, web, and
mobile apps never talk to PostHog; they are either observed server-side or send a
loopback ping this crate validates and forwards. One crate means one opt-out
chokepoint (`gate::is_enabled`), which is the whole point: a second client
somewhere else in the tree is how consent bugs happen.

Layout:
- `taxonomy` — the closed event/property table and its validator.
- `gate`     — the single opt-out chokepoint (settings + env), and the cached
               settings read the profile fields ride on.
- `identity` — the per-human `distinct_id` chain and its salted hash.
- `base`     — fixed machine/build properties added to every event.
- `profile`  — the per-install profile fields added to every event.
- `role`     — the durable "this install is a remote helper" marker.
- `queue`    — the process-global handle, bounded queue, and `capture` API.
- `capture`  — one typed emitter per event, called by the instrumentation sites.
- `client`   — the PostHog batch transport.
- `task`     — the flush and heartbeat background tasks.
- `heartbeat`— daily snapshot collection.
- `state`    — durable `analytics.state.v1` metadata row.
- `client_events` — `/api/recordClientEvent` body handling.
- `client_platform` — web client OS family from the User-Agent.
*/

pub mod base;
pub mod capture;
pub mod client;
pub mod client_events;
pub mod client_platform;
pub mod gate;
pub mod heartbeat;
pub mod identity;
pub mod profile;
pub mod queue;
pub mod role;
pub mod state;
pub mod task;
pub mod taxonomy;

pub use capture::{
    client_connected, extension_installed, extension_uninstalled, prompt_sent,
    prompt_source_for_diagnostic_input_source, session_started, ClientPlatform,
};
pub use client_events::record_client_event;
pub use gate::is_enabled;
pub use profile::ProfileSnapshot;
pub use queue::{handle, init, refresh_profile};
pub use task::{spawn_flush_task, spawn_heartbeat_task, SHUTDOWN_FLUSH_TIMEOUT};

/*
Every telemetry failure ends here, and nowhere else. Telemetry must never crash
the daemon, never block startup, and never produce a user-visible error, so
there is deliberately no path by which one of these becomes a `Result` somebody
has to handle. `debug!`-equivalent only: these lines exist for whoever is
debugging analytics, not for the user.
*/
pub(crate) fn debug_log(message: String) {
    if cfg!(debug_assertions) || std::env::var_os("GHOSTEX_TELEMETRY_DEBUG").is_some() {
        eprintln!("[telemetry] {message}");
    }
}
