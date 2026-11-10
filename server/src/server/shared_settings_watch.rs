//! `/api/readSharedSettings` and the `sharedSettingsChanged` frame: how a client with no settings
//! file of its own (the GPUI web build) reads this computer's settings and follows their changes.
//!
//! CDXC:WebGpui 2026-11-09 WHY:
//! The web page drew every setting at its default, and a change in Settings (Tool call cards, the chat and terminal widths) never reached it, because gxserver had no settings read and the page only had its `?settings=` test link. The desktop needs neither: it reads the file itself and re-checks it every second (shared_settings/service.rs). The settings file has several writers (the app, `ghostex settings` through the app, gxserver on a headless computer, Settings sync, a person's editor), so gxserver checks the file's size and time every second and sends a frame with no content when what it holds changed; the page then reads it here. The frame carries no settings, so nothing in them goes to a client that cannot read them through this route.
//! SEE-ALSO: apps/gpui-web/src/app/gx_store/shared_settings_sync.rs (the page's reader), apps/gpui-web/src/shared_settings.rs.

use std::time::SystemTime;

use super::*;
use crate::security::Principal;

pub(crate) const READ_SHARED_SETTINGS_ENDPOINT: &str = "/api/readSharedSettings";
const SHARED_SETTINGS_CHANGED_EVENT_TYPE: &str = "sharedSettingsChanged";

/// How often the settings file's size and time are compared.
const SETTINGS_FILE_CHECK: Duration = Duration::from_secs(1);

fn file_identity(paths: &crate::paths::GxserverPaths) -> Option<(SystemTime, u64)> {
    let metadata =
        std::fs::metadata(crate::shared_settings_file::shared_settings_path(paths)).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

pub(crate) fn start_shared_settings_watch(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let paths = state.paths.clone();
        let (mut identity, mut published) = tokio::task::spawn_blocking(move || {
            (
                file_identity(&paths),
                crate::shared_settings_file::read_shared_settings(&paths),
            )
        })
        .await
        .unwrap_or((None, None));
        let mut check = crate::ticks::steady_interval(SETTINGS_FILE_CHECK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = check.tick() => {}
            }
            let paths = state.paths.clone();
            let next_identity = tokio::task::spawn_blocking(move || file_identity(&paths))
                .await
                .ok()
                .flatten();
            if next_identity == identity {
                continue;
            }
            identity = next_identity;
            let paths = state.paths.clone();
            let Ok(settings) = tokio::task::spawn_blocking(move || {
                crate::shared_settings_file::read_shared_settings(&paths)
            })
            .await
            else {
                continue;
            };
            // A save that wrote the same settings again (Settings rewrites the whole file) moves
            // nothing on a client.
            if settings == published {
                continue;
            }
            published = settings;
            state.event_hub.broadcast(json!({
                "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                "serverId": state.metadata.server_id.clone(),
                "type": SHARED_SETTINGS_CHANGED_EVENT_TYPE,
            }));
        }
    });
}

/// The settings object (`{}` when nothing is configured), for this computer's own clients only:
/// the app, the web page and the CLI (the token file), and the paired phones.
pub(crate) fn handle_read_shared_settings_http(
    state: &AppState,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
) -> RoutedResponse {
    if !caller.is_owner_device() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request("Only this computer's own apps can read its settings."),
        );
    }
    let settings = crate::shared_settings_file::read_shared_settings(&state.paths)
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "settings": settings })),
    )
}
