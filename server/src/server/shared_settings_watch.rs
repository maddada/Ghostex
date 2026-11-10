//! `/api/readSharedSettings` and the `sharedSettingsChanged` frame: how a client with no settings
//! file of its own (the GPUI web build) reads this computer's settings and follows their changes.
//!
//! CDXC:WebGpui 2026-11-09 WHY:
//! The web page drew every setting at its default, and a change in Settings (Tool call cards, the chat and terminal widths) never reached it, because gxserver had no settings read and the page only had its `?settings=` test link. The desktop needs neither: it reads the file itself and re-checks it every second (shared_settings/service.rs). The settings file has several writers (the app, `ghostex settings` through the app, gxserver on a headless computer, Settings sync, a person's editor), so gxserver checks the file's size and time every second (and reads it every 30 seconds anyway, for a file replaced with the same size and time) and sends a frame with no content when what it holds changed; the page then reads it here. A refused read is not "nothing configured": it changes nothing, is retried at the next check, and answers this route with an error, so a client keeps the settings it has. The frame carries no settings, so nothing in them goes to a client that cannot read them through this route.
//! SEE-ALSO: apps/gpui-web/src/app/gx_store/shared_settings_sync.rs (the page's reader), apps/gpui-web/src/shared_settings.rs.

use std::time::SystemTime;

use super::*;
use crate::security::Principal;

pub(crate) const READ_SHARED_SETTINGS_ENDPOINT: &str = "/api/readSharedSettings";
const SHARED_SETTINGS_CHANGED_EVENT_TYPE: &str = "sharedSettingsChanged";

/// How often the settings file's size and time are compared.
const SETTINGS_FILE_CHECK: Duration = Duration::from_secs(1);
/// Every this many checks the file is read even when its size and time did not move, so a file
/// replaced with the same size and time still reaches the clients.
const CONTENT_RECHECK_EVERY: u32 = 30;

type FileIdentity = Option<(SystemTime, u64)>;

fn file_identity(paths: &crate::paths::GxserverPaths) -> FileIdentity {
    let metadata =
        std::fs::metadata(crate::shared_settings_file::shared_settings_path(paths)).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// What the watch last told the clients, and when it must read the file again.
#[derive(Default)]
struct SettingsWatch {
    /// The file's size and time at the last read that succeeded.
    identity: FileIdentity,
    /// The settings the clients were last told about; a refused read leaves them as they were.
    published: Option<Value>,
    /// The last read was refused, so the next check reads again whatever the file's size and time.
    read_failed: bool,
    checks_since_read: u32,
}

impl SettingsWatch {
    fn wants_read(&mut self, identity: FileIdentity) -> bool {
        self.checks_since_read += 1;
        identity != self.identity
            || self.read_failed
            || self.checks_since_read >= CONTENT_RECHECK_EVERY
    }

    /// Takes a read made after `identity` was taken; `true` when the clients must be told.
    fn read_done(&mut self, identity: FileIdentity, read: std::io::Result<Option<Value>>) -> bool {
        self.checks_since_read = 0;
        let Ok(settings) = read else {
            self.read_failed = true;
            return false;
        };
        self.read_failed = false;
        self.identity = identity;
        // A save that wrote the same settings again (Settings rewrites the whole file) moves
        // nothing on a client.
        if settings == self.published {
            return false;
        }
        self.published = settings;
        true
    }
}

fn read_now(paths: &crate::paths::GxserverPaths) -> (FileIdentity, std::io::Result<Option<Value>>) {
    // The size and time are taken before the read, so a save between the two is read again at
    // the next check rather than missed.
    let identity = file_identity(paths);
    (
        identity,
        crate::shared_settings_file::read_shared_settings_checked(paths),
    )
}

pub(crate) fn start_shared_settings_watch(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut watch = SettingsWatch::default();
        let paths = state.paths.clone();
        if let Ok((identity, read)) = tokio::task::spawn_blocking(move || read_now(&paths)).await {
            // What the clients read when they connect; nothing to tell them yet.
            watch.read_done(identity, read);
        }
        let mut check = crate::ticks::steady_interval(SETTINGS_FILE_CHECK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = check.tick() => {}
            }
            let paths = state.paths.clone();
            let identity = tokio::task::spawn_blocking(move || file_identity(&paths))
                .await
                .ok()
                .flatten();
            if !watch.wants_read(identity) {
                continue;
            }
            let paths = state.paths.clone();
            let Ok((identity, read)) = tokio::task::spawn_blocking(move || read_now(&paths)).await
            else {
                continue;
            };
            if watch.read_done(identity, read) {
                state.event_hub.broadcast(json!({
                    "protocolVersion": GXSERVER_PROTOCOL_VERSION,
                    "serverId": state.metadata.server_id.clone(),
                    "type": SHARED_SETTINGS_CHANGED_EVENT_TYPE,
                }));
            }
        }
    });
}

/// The settings object (`{}` when nothing is configured), for this computer's own clients only:
/// the app, the web page and the CLI (the token file), and the paired phones. A refused read is an
/// error, so a client keeps the settings it has instead of falling back to the defaults.
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
    let settings = match crate::shared_settings_file::read_shared_settings_checked(&state.paths) {
        Ok(settings) => settings
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({})),
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("Could not read the settings file: {error}"),
                },
            );
        }
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, json!({ "settings": settings })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(seconds: u64, len: u64) -> FileIdentity {
        Some((SystemTime::UNIX_EPOCH + Duration::from_secs(seconds), len))
    }

    fn refused() -> std::io::Result<Option<Value>> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }

    #[test]
    fn a_refused_read_keeps_the_last_settings_and_is_retried_with_the_same_size_and_time() {
        let mut watch = SettingsWatch::default();
        let saved = json!({ "terminalViewWidthPercent": 60 });
        watch.read_done(identity(1, 10), Ok(Some(saved.clone())));
        assert!(!watch.wants_read(identity(1, 10)));
        // Briefly unreadable: nothing is told, and nothing turns into the defaults.
        assert!(watch.wants_read(identity(2, 10)));
        assert!(!watch.read_done(identity(2, 10), refused()));
        assert_eq!(watch.published, Some(saved.clone()));
        // Back with the size and time it had before the failure: read again, nothing moved.
        assert!(watch.wants_read(identity(2, 10)));
        assert!(!watch.read_done(identity(2, 10), Ok(Some(saved.clone()))));
        assert_eq!(watch.published, Some(saved));
        assert!(!watch.wants_read(identity(2, 10)));
    }

    #[test]
    fn a_file_replaced_with_the_same_size_and_time_is_caught_by_the_content_check() {
        let mut watch = SettingsWatch::default();
        watch.read_done(identity(1, 10), Ok(Some(json!({ "a": 1 }))));
        let reads = (0..CONTENT_RECHECK_EVERY)
            .filter(|_| watch.wants_read(identity(1, 10)))
            .count();
        assert_eq!(reads, 1);
        assert!(watch.read_done(identity(1, 10), Ok(Some(json!({ "a": 2 })))));
    }

    #[cfg(unix)]
    #[test]
    fn a_refused_read_is_told_apart_from_a_missing_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        let path = crate::shared_settings_file::shared_settings_path(&paths);
        assert!(matches!(
            crate::shared_settings_file::read_shared_settings_checked(&paths),
            Ok(None)
        ));
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(&path, br#"{"a":1}"#).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let read = crate::shared_settings_file::read_shared_settings_checked(&paths);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        assert!(read.is_err());
    }
}
