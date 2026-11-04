//! Tells connected clients when this machine's Spaces switch flips, so a client viewing this
//! machine shows or hides its Spaces within seconds (`crate::sidebar_spaces::read_sidebar_spaces_enabled`).
//!
//! CDXC:Spaces 2026-10-06 WHY:
//! The switch lives in the settings file, which the desktop's Settings page and `ghostex settings`
//! write directly, so gxserver never hears of the write. It reads the file every few seconds, as
//! the Auto Sleep sweep does, and broadcasts a `sidebarSpacesChanged` frame carrying the current
//! Spaces document and the new switch when the switch moved. Snapshots carry the switch too, so a
//! client that connects later needs no frame.

use super::*;

/// How often the settings file is read.
const SETTINGS_POLL: Duration = Duration::from_secs(5);

pub(crate) fn start_sidebar_spaces_switch_watch(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut published = crate::sidebar_spaces::read_sidebar_spaces_enabled(&state.paths);
        let mut poll = crate::ticks::steady_interval(SETTINGS_POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            let enabled = crate::sidebar_spaces::read_sidebar_spaces_enabled(&state.paths);
            if enabled == published {
                continue;
            }
            let publish_state = state.clone();
            let sent = tokio::task::spawn_blocking(move || {
                publish_sidebar_spaces_switch(&publish_state, enabled)
            })
            .await;
            // A pass that could not publish tries again on the next read.
            if matches!(sent, Ok(Ok(()))) {
                published = enabled;
            }
        }
    });
}

fn publish_sidebar_spaces_switch(
    state: &AppState,
    enabled: bool,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let _event_sequence = lock_presentation_event_sequence(state)?;
    let spaces = crate::sidebar_spaces::read_sidebar_spaces(&db)?;
    let revision = increment_presentation_revision(&db)?;
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "sidebarSpaces": spaces,
        "sidebarSpacesEnabled": enabled,
        "type": "sidebarSpacesChanged",
    }));
    Ok(())
}
