use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::auth::DeviceApiTokens;
use crate::paths::GxserverPaths;
use crate::tailcat::{
    apply_tailcat_state_update, read_tailcat_status_payload, TailcatRuntime, TailcatStateUpdate,
};

use super::authorized_keys::{remove_authorized_key, AdministratorApprovalDeclined};
use super::repository::{RemotePairedDeviceRecord, RemotePairingRepository};
use crate::push::{remove_device_push_registrations, PushRegistrationStore};

/// Wire shape of `GxserverPairedDevice` in `packages/shared/gxserver-protocol-health.ts`.
pub fn paired_device_json(device: &RemotePairedDeviceRecord) -> Value {
    json!({
        "id": device.id,
        "name": device.name,
        "platform": device.platform,
        "pairedAt": device.paired_at,
        "lastSeenAt": device.last_seen_at,
        "sshKeyFingerprint": device.ssh_key_fingerprint,
        "tailcatClientKey": device.tailcat_client_key,
    })
}

pub fn list_paired_devices_json(db: &Connection) -> Result<Value> {
    let devices = RemotePairingRepository::new(db).list_devices()?;
    Ok(Value::Array(
        devices.iter().map(paired_device_json).collect(),
    ))
}

/*
CDXC:RemotePairing 2026-10-31 WHY:
Unpairing reverses everything pairing did, key line first (its API token goes with the row, CDXC:RemotePairing 2026-10-25 in `auth.rs`): on a Windows administrator account removing it needs a UAC prompt the user can decline, and a device whose row went first would keep SSH access with nothing left to click Remove on. With the key first, a declined or failed removal leaves the device listed so Remove can be retried, and a failure after it leaves only a row without access. Then the phone's push registration, so a failure to delete it also leaves the device listed for another Remove instead of a token nothing links back to; the push store lock is held until the row is gone so a registration racing the unpair cannot add the token back. Then the row, then its tailcat allow-list entry. Supersedes the 2026-10-08 order, which had no push step. Returns the removed record, or `None` when no such device exists.
SEE-ALSO: server/src/push/registration.rs (a registration names its paired device).
*/
pub fn remove_paired_device(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    device_tokens: &DeviceApiTokens,
    device_id: &str,
) -> Result<Option<RemotePairedDeviceRecord>> {
    remove_paired_device_using(
        paths,
        db,
        tailcat_runtime,
        device_tokens,
        device_id,
        remove_authorized_key,
    )
}

/// `remove_paired_device` with the key-line removal passed in, so tests can stand in for the
/// user's real `authorized_keys` file.
pub(crate) fn remove_paired_device_using(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    device_tokens: &DeviceApiTokens,
    device_id: &str,
    remove_key: impl FnOnce(&str) -> Result<bool>,
) -> Result<Option<RemotePairedDeviceRecord>> {
    let repository = RemotePairingRepository::new(db);
    let Some(device) = repository.find_device(device_id)? else {
        return Ok(None);
    };
    remove_key(&device.id).map_err(|error| {
        match error.downcast_ref::<AdministratorApprovalDeclined>() {
            Some(declined) => anyhow::anyhow!(
                "{} is still paired: Windows asked for administrator approval to remove its key from {}, and the prompt was declined. Click Remove again and choose Yes.",
                device.name,
                declined.keys_file.display()
            ),
            None => error,
        }
    })?;
    let push_registrations = PushRegistrationStore::open(paths);
    remove_device_push_registrations(&push_registrations, &device.id)?;
    if repository.remove_device(&device.id)?.is_none() {
        return Ok(None);
    }
    drop(push_registrations);
    reload_device_api_tokens(db, device_tokens)?;
    if let Some(client_key) = device.tailcat_client_key.as_deref() {
        remove_tailcat_client_key(paths, db, tailcat_runtime, client_key)?;
    }
    Ok(Some(device))
}

/*
CDXC:RemotePairing 2026-11-05 WHY:
A phone that pairs again replaces its earlier entries, so it is listed once (desktop Settings and the phone). The phone is recognized by its tailcat client key: it lives in the app's private storage, outside backups, and stays the same across pairings, while the SSH key is generated anew each time and the device name ("iPhone") is shared by many phones. A phone without a client key (an app build from before Easy Connect allow-listing) cannot be told apart from another phone and is never merged. Each earlier entry is revoked like Remove does it, key line first and then the row (its API token goes with it), but the tailcat key stays on the allow-list because the new entry uses it. Every earlier entry with the key goes, so a list that already holds duplicates collapses to one on the phone's next pairing. A key line that cannot be removed (a declined Windows administrator prompt) keeps that entry listed so Remove can retry it; the new pairing still succeeds. Runs after the new entry is saved, so a failure here never costs the phone its new access. Returns the replaced entries.
*/
pub fn replace_earlier_pairings_of_same_phone(
    db: &Connection,
    new_device_id: &str,
    tailcat_client_key: Option<&str>,
    mut remove_key_line: impl FnMut(&str) -> Result<bool>,
    mut on_key_line_error: impl FnMut(&RemotePairedDeviceRecord, &anyhow::Error),
) -> Result<Vec<RemotePairedDeviceRecord>> {
    let Some(client_key) = tailcat_client_key else {
        return Ok(Vec::new());
    };
    let repository = RemotePairingRepository::new(db);
    let mut replaced = Vec::new();
    for device in repository.list_devices()? {
        if device.id == new_device_id || device.tailcat_client_key.as_deref() != Some(client_key) {
            continue;
        }
        if let Err(error) = remove_key_line(&device.id) {
            on_key_line_error(&device, &error);
            continue;
        }
        if let Some(removed) = repository.remove_device(&device.id)? {
            replaced.push(removed);
        }
    }
    Ok(replaced)
}

/// Mirrors the stored device token hashes into the set every request is checked against.
pub fn reload_device_api_tokens(db: &Connection, device_tokens: &DeviceApiTokens) -> Result<()> {
    device_tokens.replace(RemotePairingRepository::new(db).list_api_token_hashes()?);
    Ok(())
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
    let allowed = read_tailcat_status_payload(paths, db, tailcat_runtime).allowed_client_keys;
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

/// tailcat's `--allow` entry that admits no client (`tailcat serve --allow=none`).
const TAILCAT_ALLOW_NONE: &str = "none";

/*
CDXC:RemotePairing 2026-11-05 WHY:
A phone that pairs twice keeps one tailcat client key but gets two device rows (each with its own SSH key and API token). Removing the old row used to drop that shared key from the allow-list and lock the phone out although its newer row still needed it, so the key now leaves the list only when no remaining paired device uses it. The caller has already deleted the row being removed, so "remaining" is simply every row left in the table. Since the same date a new pairing replaces the phone's earlier rows (`replace_earlier_pairings_of_same_phone`), so this guard now covers a list saved before that and any row whose key line could not be removed.
*/
fn tailcat_client_key_in_use(db: &Connection, client_key: &str) -> Result<bool> {
    Ok(RemotePairingRepository::new(db)
        .list_devices()?
        .iter()
        .any(|device| device.tailcat_client_key.as_deref() == Some(client_key)))
}

/// The allow-list after `client_key` is dropped, or `None` when it is not listed.
fn allow_list_without_client_key(allowed: Vec<String>, client_key: &str) -> Option<Vec<String>> {
    if !allowed.iter().any(|key| key == client_key) {
        return None;
    }
    let mut allowed_client_keys: Vec<String> = allowed
        .into_iter()
        .filter(|key| key != client_key)
        .collect();
    // "Only paired devices can connect" is a non-empty list, and an empty one admits anyone with
    // the code, so removing the last listed phone used to turn it off silently (security review
    // 2026-11-05). The list keeps tailcat's `none` instead: no client is allowed until the next
    // phone pairs (the pairing window lifts the list) and is added beside it.
    if allowed_client_keys.is_empty() {
        allowed_client_keys.push(TAILCAT_ALLOW_NONE.to_string());
    }
    Some(allowed_client_keys)
}

fn remove_tailcat_client_key(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    client_key: &str,
) -> Result<bool> {
    if tailcat_client_key_in_use(db, client_key)? {
        return Ok(false);
    }
    let allowed = read_tailcat_status_payload(paths, db, tailcat_runtime).allowed_client_keys;
    let Some(allowed_client_keys) = allow_list_without_client_key(allowed, client_key) else {
        return Ok(false);
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn database() -> Connection {
        let db = Connection::open_in_memory().expect("in-memory database");
        db.execute_batch(
            "CREATE TABLE remote_paired_devices (
               id TEXT PRIMARY KEY CHECK (id <> ''),
               name TEXT NOT NULL,
               platform TEXT NOT NULL,
               sshKeyFingerprint TEXT NOT NULL,
               tailcatClientKey TEXT,
               pairedAt TEXT NOT NULL,
               lastSeenAt TEXT
             );",
        )
        .expect("devices table");
        db.execute_batch(include_str!(
            "../storage/migrations/0059_paired_device_api_token.sql"
        ))
        .expect("api token column");
        db
    }

    fn pair(db: &Connection, id: &str, client_key: Option<&str>) {
        RemotePairingRepository::new(db)
            .insert_device(&RemotePairedDeviceRecord {
                id: id.to_string(),
                name: format!("Phone {id}"),
                platform: "ios".to_string(),
                ssh_key_fingerprint: format!("SHA256:{id}"),
                tailcat_client_key: client_key.map(str::to_string),
                api_token_hash: None,
                paired_at: "2026-11-05T00:00:00Z".to_string(),
                last_seen_at: None,
            })
            .expect("insert device");
    }

    #[test]
    fn removing_one_of_two_entries_sharing_a_key_keeps_the_key() {
        let db = database();
        pair(&db, "old", Some("key-a"));
        pair(&db, "new", Some("key-a"));
        RemotePairingRepository::new(&db)
            .remove_device("old")
            .expect("remove");
        assert!(tailcat_client_key_in_use(&db, "key-a").unwrap());
    }

    #[test]
    fn removing_the_last_entry_for_a_key_drops_it() {
        let db = database();
        pair(&db, "old", Some("key-a"));
        pair(&db, "other", Some("key-b"));
        RemotePairingRepository::new(&db)
            .remove_device("old")
            .expect("remove");
        assert!(!tailcat_client_key_in_use(&db, "key-a").unwrap());
        assert!(tailcat_client_key_in_use(&db, "key-b").unwrap());
        assert_eq!(
            allow_list_without_client_key(keys(&["key-a", "key-b"]), "key-a"),
            Some(keys(&["key-b"]))
        );
    }

    #[test]
    fn removing_the_last_listed_key_leaves_none_so_the_list_never_goes_empty() {
        assert_eq!(
            allow_list_without_client_key(keys(&["key-a"]), "key-a"),
            Some(keys(&[TAILCAT_ALLOW_NONE]))
        );
    }

    #[test]
    fn an_unlisted_key_leaves_the_allow_list_alone() {
        assert_eq!(
            allow_list_without_client_key(keys(&["none"]), "key-a"),
            None
        );
        assert_eq!(allow_list_without_client_key(Vec::new(), "key-a"), None);
    }

    #[test]
    fn a_device_without_a_key_never_counts_as_using_one() {
        let db = database();
        pair(&db, "legacy", None);
        assert!(!tailcat_client_key_in_use(&db, "key-a").unwrap());
    }
}
