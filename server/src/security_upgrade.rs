//! The one-time 11.0.1 security notice: which phones and computers that use Easy Connect must pair
//! again, decided once from the stored pairing data and shown once.
//!
//! At its first start on 11.0.1 or later, gxserver writes `<gxserver state>/security/upgrade-11.0.1.json`
//! before anything else touches the pairing data: the paired phones (every phone paired through
//! Easy Connect) and the remote computers this computer reaches through Easy Connect that are not
//! turned off. A computer with neither gets an empty record, so a phone paired later never brings
//! the notice up. `/api/securityUpgradeNotice` reads it (`op: "read"`) and marks it seen
//! (`op: "dismiss"`); the desktop shows it once when its gxserver comes up.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (GitHub #495, rollout): "on first start of 11.0.1, a one-time notice explaining Ghostex got more secure and asking to re-pair machines/phones that use Easy Connect, shown ONLY to users with active Easy Connect machines/devices."
//! CDXC:Security 2026-11-08 WHY: the pairing redesign removes old phone pairings when it starts, so the list is taken before any of that runs (`snapshot_before_pairing_changes` is called right after gxserver's storage opens); code that removes old pairings must run after it.
//! SEE-ALSO: apps/desktop/src/app/gx_store/security_notice.rs (the desktop's notice).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::GxserverPaths;

pub(crate) const SECURITY_UPGRADE_NOTICE_ENDPOINT: &str = "/api/securityUpgradeNotice";
const NOTICE_FILE: &str = "upgrade-11.0.1.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpgradeNotice {
    /// Names of the phones paired through Easy Connect.
    pub(crate) phones: Vec<String>,
    /// Names of the computers this one reaches through Easy Connect.
    pub(crate) computers: Vec<String>,
    pub(crate) seen: bool,
    pub(crate) taken_at_ms: i64,
}

impl UpgradeNotice {
    pub(crate) fn should_show(&self) -> bool {
        !self.seen && !(self.phones.is_empty() && self.computers.is_empty())
    }
}

fn notice_path(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("security").join(NOTICE_FILE)
}

/// Takes the list once, at the first start that has this code. Never fails gxserver's start.
pub(crate) fn snapshot_before_pairing_changes(paths: &GxserverPaths) -> Result<()> {
    let path = notice_path(paths);
    if path.exists() {
        return Ok(());
    }
    let notice = UpgradeNotice {
        phones: paired_phone_names(&paths.state_db_file),
        computers: easy_connect_computer_names(
            crate::shared_settings_file::read_shared_settings(paths).as_ref(),
        ),
        seen: false,
        taken_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    write(&path, &notice)
}

/// The paired phones in the state database, read before its migrations or anything else run. A
/// database that is not there yet (a new install) has none.
fn paired_phone_names(db_file: &Path) -> Vec<String> {
    if !db_file.exists() {
        return Vec::new();
    }
    let Ok(db) =
        rusqlite::Connection::open_with_flags(db_file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return Vec::new();
    };
    crate::remote_access::repository::RemotePairingRepository::new(&db)
        .list_devices()
        .map(|devices| devices.into_iter().map(|device| device.name).collect())
        .unwrap_or_default()
}

/// The remote computers saved with the Easy Connect transport and not turned off.
pub(crate) fn easy_connect_computer_names(settings: Option<&Value>) -> Vec<String> {
    settings
        .and_then(|settings| settings.get("remoteMachines"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|machine| {
            machine.get("transport").and_then(Value::as_str) == Some("easyConnect")
                && machine.get("disabled").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|machine| machine.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn read(paths: &GxserverPaths) -> UpgradeNotice {
    std::fs::read_to_string(notice_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| UpgradeNotice {
            seen: true,
            ..UpgradeNotice::default()
        })
}

pub(crate) fn dismiss(paths: &GxserverPaths) -> Result<UpgradeNotice> {
    let mut notice = read(paths);
    notice.seen = true;
    write(&notice_path(paths), &notice)?;
    Ok(notice)
}

fn write(path: &Path, notice: &UpgradeNotice) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create the security folder")?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(notice)?)
        .context("write the security upgrade notice")
}

/// The `/api/securityUpgradeNotice` route.
#[inline(never)]
pub(crate) fn handle_http(
    paths: &GxserverPaths,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> crate::server::RoutedResponse {
    use axum::http::StatusCode;
    let params = crate::domain::read_domain_rpc_params(body).unwrap_or_default();
    match handle_endpoint(paths, &params) {
        Ok(result) => crate::server::routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(error) => crate::server::routed_json(
            Some(endpoint_path),
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::protocol::rpc_error("internalError", format!("{error:#}"), Some(request_id)),
        ),
    }
}

/// `op: "read"` (default): `show`, `phones`, `computers`. `op: "dismiss"`: marks it seen.
pub(crate) fn handle_endpoint(
    paths: &GxserverPaths,
    params: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let notice = match params.get("op").and_then(Value::as_str) {
        Some("dismiss") => dismiss(paths)?,
        _ => read(paths),
    };
    Ok(serde_json::json!({
        "show": notice.should_show(),
        "phones": notice.phones,
        "computers": notice.computers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn paths() -> (tempfile::TempDir, GxserverPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(dir.path().to_path_buf()));
        (dir, paths)
    }

    #[test]
    fn only_active_easy_connect_computers_count() {
        let names = easy_connect_computer_names(Some(&json!({
            "remoteMachines": [
                { "name": "Studio", "transport": "easyConnect", "easyConnectAddress": "tc1x" },
                { "name": "Old box", "transport": "easyConnect", "disabled": true },
                { "name": "Server", "sshHost": "server.local" },
            ]
        })));
        assert_eq!(names, vec!["Studio".to_string()]);
        assert!(easy_connect_computer_names(None).is_empty());
    }

    #[test]
    fn a_new_install_never_shows_it() {
        let (_dir, paths) = paths();
        snapshot_before_pairing_changes(&paths).unwrap();
        assert!(!read(&paths).should_show());
    }

    #[test]
    fn it_shows_once_and_the_first_list_is_kept() {
        let (_dir, paths) = paths();
        write(
            &notice_path(&paths),
            &UpgradeNotice {
                phones: vec!["Pixel".into()],
                ..UpgradeNotice::default()
            },
        )
        .unwrap();
        // A later start keeps the list taken at the first one.
        snapshot_before_pairing_changes(&paths).unwrap();
        let shown = handle_endpoint(&paths, &serde_json::Map::new()).unwrap();
        assert_eq!(shown["show"], true);
        assert_eq!(shown["phones"], json!(["Pixel"]));
        let mut dismiss_params = serde_json::Map::new();
        dismiss_params.insert("op".into(), json!("dismiss"));
        handle_endpoint(&paths, &dismiss_params).unwrap();
        assert_eq!(
            handle_endpoint(&paths, &serde_json::Map::new()).unwrap()["show"],
            false
        );
    }

    #[test]
    fn paired_phones_come_from_the_state_database() {
        let (_dir, paths) = paths();
        crate::storage::initialize_gxserver_storage(&paths).unwrap();
        {
            let db = crate::storage::open_gxserver_database(&paths).unwrap();
            crate::remote_access::repository::RemotePairingRepository::new(&db)
                .insert_device(
                    &crate::remote_access::repository::RemotePairedDeviceRecord {
                        id: "d1".into(),
                        name: "Pixel 9".into(),
                        platform: "android".into(),
                        ssh_key_fingerprint: "SHA256:x".into(),
                        tailcat_client_key: None,
                        api_token_hash: None,
                        paired_at: "2026-11-01T00:00:00Z".into(),
                        last_seen_at: None,
                    },
                )
                .unwrap();
        }
        snapshot_before_pairing_changes(&paths).unwrap();
        let notice = read(&paths);
        assert_eq!(notice.phones, vec!["Pixel 9".to_string()]);
        assert!(notice.should_show());
    }
}
