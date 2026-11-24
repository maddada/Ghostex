/*
CDXC:Telemetry 2026-08-26:
Anonymous PostHog analytics. Documentation: root `ANALYTICS.md`.

ALL analytics egress in Ghostex happens here, in gxserver. The desktop, web, and
mobile apps never talk to PostHog; they are either observed server-side or send a
loopback ping this crate validates and forwards. One crate means one opt-out
chokepoint (`gate::is_enabled`), which is the whole point: a second client
somewhere else in the tree is how consent bugs happen.

Layout:
- `taxonomy`: the closed event/property table and its validator.
- `gate`: the single opt-out chokepoint (settings + env + first-run onboarding hold),
  and the cached settings read the profile fields ride on.
- `identity`: the random per-install UUIDv4 (`analytics-install-id`).
- `base`: fixed machine/build properties added to every event.
- `profile`: the per-install profile fields added to every event.
- `role`: the durable "this install is a remote helper" marker.
- `queue`: the process-global handle, bounded queue, preview builder, and `capture` API.
- `capture`: one typed emitter per event, called by the instrumentation sites.
- `client`: the PostHog batch transport and `GHOSTEX_TELEMETRY_DEBUG=1` payload logger.
- `task`: the flush and activity-gated heartbeat background tasks plus opt-out wipe.
- `heartbeat`: daily snapshot collection.
- `state`: durable `analytics.state.v1` metadata row.
- `client_events`: `/api/recordClientEvent` body handling.
- `client_platform`: web client OS family from the User-Agent.
- `extensions`: the built-in / official Store extension whitelist.
- `settings_snapshot`: the daily install snapshot (settings, Linux, accounts, Boosts).
*/

pub mod base;
pub mod capture;
pub mod client;
pub mod client_events;
pub mod client_platform;
pub mod extensions;
pub mod gate;
pub mod heartbeat;
pub mod identity;
pub mod profile;
pub mod queue;
pub mod role;
pub mod settings_snapshot;
pub mod state;
pub mod task;
pub mod taxonomy;
#[cfg(test)]
mod tests;

pub use capture::{
    boosts_used, client_connected, extension_installed, extension_toggled, extension_uninstalled,
    install_weekly_count, mobile_keep_awake_connected, onboarding_step, prompt_sent,
    prompt_source_for_diagnostic_input_source, reliability_error, reliability_error_for_session,
    resolve_install_session_modes, session_mode_state, session_mode_switched, session_started,
    ClientPlatform, InstallSessionModes, LiveSessionModeCounts,
};
pub use client_events::record_client_event;
pub use gate::{
    is_enabled, is_hard_opted_out, is_onboarding_held, mode, status_reason, TelemetryMode,
    TelemetryStatusReason,
};
pub use profile::ProfileSnapshot;
pub use queue::{
    build_preview_event, build_weekly_count_preview_event, handle, init, refresh_profile,
};
pub(crate) use task::note_accounts_discovered;
pub use task::{purge_on_opt_out, spawn_flush_task, spawn_heartbeat_task, SHUTDOWN_FLUSH_TIMEOUT};

/// True when `GHOSTEX_TELEMETRY_DEBUG=1` (or `true`) is set, enabling full
/// outgoing JSON batch payload logging to stderr for inspection.
pub(crate) fn debug_payload_logging_enabled() -> bool {
    gate::env_is_truthy("GHOSTEX_TELEMETRY_DEBUG")
}

/*
Every telemetry failure ends here, and nowhere else. Telemetry must never crash
the daemon, never block startup, and never produce a user-visible error, so
there is deliberately no path by which one of these becomes a `Result` somebody
has to handle. `debug!`-equivalent only: these lines exist for whoever is
debugging analytics, not for the user.
*/
pub(crate) fn debug_log(message: String) {
    if cfg!(debug_assertions) || debug_payload_logging_enabled() {
        eprintln!("[telemetry] {message}");
    }
}
