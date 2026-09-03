use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::paths::GxserverPaths;
use crate::tailcat::{
    apply_tailcat_state_update, read_tailcat_status_payload, TailcatRuntime, TailcatStateUpdate,
};

use super::authorized_keys::remove_authorized_key;
use super::repository::{RemotePairedDeviceRecord, RemotePairingRepository};

/// Wire shape of `GxserverPairedDevice` in `packages/shared/gxserver-protocol.ts`.
pub fn paired_device_json(device: &RemotePairedDeviceRecord) -> Value {
    json!({
        "id": device.id,
        "name": device.name,
        "platform": device.platform,
        "pairedAt": device.paired_at,
        "lastSeenAt": device.last_seen_at,
        "sshKeyFingerprint": device.ssh_key_fingerprint,
    })
}

pub fn list_paired_devices_json(db: &Connection) -> Result<Value> {
    let devices = RemotePairingRepository::new(db).list_devices()?;
    Ok(Value::Array(
        devices.iter().map(paired_device_json).collect(),
    ))
}

/*
CDXC:RemotePairing 2026-09-03:
Unpairing reverses everything pairing did, in the order that leaves the
least behind if a later step fails: the row goes first so the device stops
being listed, then its `authorized_keys` line (the actual access), then its
tailcat allow-list entry. Returns the removed record, or `None` when no such
device exists.
*/
pub fn remove_paired_device(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    device_id: &str,
) -> Result<Option<RemotePairedDeviceRecord>> {
    let Some(device) = RemotePairingRepository::new(db).remove_device(device_id)? else {
        return Ok(None);
    };
    remove_authorized_key(&device.id)?;
    if let Some(client_key) = device.tailcat_client_key.as_deref() {
        remove_tailcat_client_key(paths, db, tailcat_runtime, client_key)?;
    }
    Ok(Some(device))
}

pub fn touch_paired_device_seen(
    db: &Connection,
    device_id: &str,
) -> Result<Option<RemotePairedDeviceRecord>> {
    let repository = RemotePairingRepository::new(db);
    if repository.find_device(device_id)?.is_none() {
        return Ok(None);
    }
    repository.touch_device_seen(device_id)
}

/// Adds `client_key` to the tailcat allow-list only while that list is
/// already non-empty: an empty list means "allow any client" and turning it
/// into a one-entry list would lock every other client out.
pub fn add_tailcat_client_key_when_allow_listed(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    client_key: &str,
) -> Result<bool> {
    let allowed = read_tailcat_status_payload(db, tailcat_runtime).allowed_client_keys;
    if allowed.is_empty() || allowed.iter().any(|key| key == client_key) {
        return Ok(false);
    }
    let mut allowed_client_keys = allowed;
    allowed_client_keys.push(client_key.to_string());
    apply_tailcat_state_update(
        paths,
        db,
        tailcat_runtime,
        TailcatStateUpdate::SetAllowedClientKeys {
            allowed_client_keys,
        },
    )?;
    Ok(true)
}

fn remove_tailcat_client_key(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    client_key: &str,
) -> Result<bool> {
    let allowed = read_tailcat_status_payload(db, tailcat_runtime).allowed_client_keys;
    if !allowed.iter().any(|key| key == client_key) {
        return Ok(false);
    }
    let allowed_client_keys = allowed
        .into_iter()
        .filter(|key| key != client_key)
        .collect();
    apply_tailcat_state_update(
        paths,
        db,
        tailcat_runtime,
        TailcatStateUpdate::SetAllowedClientKeys {
            allowed_client_keys,
        },
    )?;
    Ok(true)
}
