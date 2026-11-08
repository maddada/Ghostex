use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::auth::{DeviceApiTokens, DeviceTokenEntry};
use crate::paths::GxserverPaths;
use crate::security::{Principal, SecurityEvent, SecurityState};
use crate::tailcat::{
    apply_tailcat_state_update, read_tailcat_status_payload, TailcatRuntime, TailcatStateUpdate,
};

use super::authorized_keys::{remove_authorized_key, AdministratorApprovalDeclined};
use super::repository::{PairedDeviceStatus, RemotePairedDeviceRecord, RemotePairingRepository};
use crate::push::{remove_device_push_registrations, PushRegistrationStore};

/// Wire shape of `GxserverPairedDevice` in `packages/shared/gxserver-protocol-health.ts`.
/// `status` is `paired`, or `legacy` for a phone paired before 11.0.1 that must pair again (it
/// has no access left; Remove takes it off the list).
pub fn paired_device_json(device: &RemotePairedDeviceRecord) -> Value {
    json!({
        "id": device.id,
        "name": device.name,
        "platform": device.platform,
        "pairedAt": device.paired_at,
        "lastSeenAt": device.last_seen_at,
        "sshKeyFingerprint": device.ssh_key_fingerprint,
        "tailcatClientKey": device.tailcat_client_key,
        "status": device.status.as_str(),
        "deviceKeyId": device.device_key.as_ref().map(|key| key.key_id.as_str()),
        "tokenExpiresAt": device.api_token_expires_at,
    })
}

/*
CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495 rollout): "old pairings stop working in 11.0.1" and a one-time notice asks users
with active Easy Connect devices to pair them again. Migration 0062 made every earlier row `legacy`
and dropped its token; at each start gxserver also removes those rows' unrestricted SSH key lines
from the file this account's sshd reads, without ever showing a Windows administrator prompt (a
line in `administrators_authorized_keys` waits for the user's Remove, which may prompt). The rows
stay listed as "pair again" until the phone pairs again (the new pairing replaces them, matched by
its tailcat key) or the user removes them. Returns how many lines were removed.
*/
pub fn retire_legacy_pairings(db: &Connection) -> Result<usize> {
    retire_legacy_pairings_using(
        db,
        super::authorized_keys::remove_authorized_key_without_prompt,
    )
}

pub(crate) fn retire_legacy_pairings_using(
    db: &Connection,
    mut remove_quietly: impl FnMut(&str) -> Result<Option<bool>>,
) -> Result<usize> {
    let repository = RemotePairingRepository::new(db);
    let mut removed = 0;
    for device in repository.list_devices()? {
        if device.status != PairedDeviceStatus::Legacy || device.legacy_key_line_removed {
            continue;
        }
        match remove_quietly(&device.id) {
            Ok(Some(was_there)) => {
                removed += usize::from(was_there);
                repository.mark_legacy_key_line_removed(&device.id)?;
            }
            // Needs a Windows administrator prompt, or the file couldn't be read: the line stays
            // until the user removes the row.
            Ok(None) | Err(_) => {}
        }
    }
    Ok(removed)
}

/// Whether this computer has any Easy Connect devices, paired before 11.0.1 (`legacy`, which
/// must pair again) or since. For the one-time 11.0.1 "pair your devices again" notice: show it
/// only when this is true.
pub fn has_active_easy_connect_devices(db: &Connection) -> Result<bool> {
    Ok(!RemotePairingRepository::new(db).list_devices()?.is_empty())
}

/// The names of the devices paired before 11.0.1 that must pair again, newest first.
pub fn easy_connect_devices_to_pair_again(db: &Connection) -> Result<Vec<String>> {
    Ok(RemotePairingRepository::new(db)
        .list_devices()?
        .into_iter()
        .filter(|device| device.status == PairedDeviceStatus::Legacy)
        .map(|device| device.name)
        .collect())
}

/// Removes the approver key a device enrolled when its pairing was approved (revoking a device
/// takes away its power to approve too).
pub fn forget_approver_key(
    security: &SecurityState,
    device: &RemotePairedDeviceRecord,
    by: &Principal,
) {
    let mut key_ids: Vec<String> = security
        .approver_keys
        .list()
        .into_iter()
        .filter(|key| key.device_id == device.id)
        .map(|key| key.key_id)
        .collect();
    key_ids.extend(device.approver_key_id.clone());
    key_ids.sort();
    key_ids.dedup();
    for key_id in key_ids {
        if let Some(key) = security.approver_keys.remove(&key_id) {
            security.publish(SecurityEvent::ApproverKeyRemoved {
                key,
                by: by.clone(),
            });
        }
    }
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
    paths: &GxserverPaths,
    db: &Connection,
    new_device_id: &str,
    tailcat_client_key: Option<&str>,
    mut remove_key_line: impl FnMut(&str) -> Result<bool>,
    mut on_kept: impl FnMut(&RemotePairedDeviceRecord, &anyhow::Error),
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
            on_kept(&device, &error);
            continue;
        }
        // As in Remove, the entry's push registrations go under the push store's lock, held until
        // its row is deleted, so the phone can register again under its new pairing. A store that
        // cannot be cleaned keeps the entry listed, like a key line that cannot be removed.
        let push_registrations = PushRegistrationStore::open(paths);
        if let Err(error) = remove_device_push_registrations(&push_registrations, &device.id) {
            on_kept(&device, &error.into());
            continue;
        }
        if let Some(removed) = repository.remove_device(&device.id)? {
            replaced.push(removed);
        }
    }
    Ok(replaced)
}

/// Mirrors the stored device tokens (paired v2 devices only) into the map every request is
/// checked against, with the device each names and when it lapses.
pub fn reload_device_api_tokens(db: &Connection, device_tokens: &DeviceApiTokens) -> Result<()> {
    let entries = RemotePairingRepository::new(db)
        .list_api_tokens()?
        .into_iter()
        .map(|(hash, device_id, expires_at)| {
            let expires_at_ms = expires_at
                .as_deref()
                .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
                // An unreadable expiry lapses at once rather than never.
                .map_or_else(
                    || expires_at.as_ref().map(|_| 0),
                    |time| Some(time.timestamp_millis().max(0) as u64),
                );
            (
                hash,
                DeviceTokenEntry {
                    device_id: Some(device_id),
                    expires_at_ms,
                },
            )
        });
    device_tokens.replace_entries(entries);
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

/// Adds a newly paired phone's `client_key` to the tailcat allow-list. Since an empty list means
/// closed (CDXC:RemotePairing 2026-11-08 in tailcat/supervisor.rs) the key always goes on, and a
/// `none` placeholder makes way for it. Returns whether the list changed.
pub fn add_tailcat_client_key_when_allow_listed(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    client_key: &str,
) -> Result<bool> {
    let allowed = read_tailcat_status_payload(paths, db, tailcat_runtime).allowed_client_keys;
    let Some(allowed_client_keys) = allow_list_with_client_key(allowed, client_key) else {
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

/// The allow-list with `client_key` added (and `none` dropped), or `None` when it is listed already.
fn allow_list_with_client_key(allowed: Vec<String>, client_key: &str) -> Option<Vec<String>> {
    if allowed.iter().any(|key| key == client_key) {
        return None;
    }
    let mut keys: Vec<String> = allowed
        .into_iter()
        .filter(|key| key != TAILCAT_ALLOW_NONE)
        .collect();
    keys.push(client_key.to_string());
    Some(keys)
}

use crate::tailcat::TAILCAT_ALLOW_NONE;

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
    // An empty list is closed since 2026-11-08 (tailcat/supervisor.rs); `none` keeps saying so
    // plainly to an older gxserver that still read an empty list as open (security review
    // 2026-11-05). The next phone that pairs replaces it.
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
        db.execute_batch(include_str!(
            "../storage/migrations/0062_pairing_v2_device_keys.sql"
        ))
        .expect("pairing v2 columns");
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
                ..Default::default()
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
    fn a_paired_phone_always_goes_on_the_allow_list_even_an_empty_one() {
        assert_eq!(
            allow_list_with_client_key(Vec::new(), "key-a"),
            Some(keys(&["key-a"]))
        );
        assert_eq!(
            allow_list_with_client_key(keys(&[TAILCAT_ALLOW_NONE]), "key-a"),
            Some(keys(&["key-a"]))
        );
        assert_eq!(allow_list_with_client_key(keys(&["key-a"]), "key-a"), None);
    }

    #[test]
    fn an_empty_allow_list_is_served_closed_except_in_the_pairing_window() {
        use crate::tailcat::served_allow_list;
        assert_eq!(served_allow_list(&[], false), keys(&[TAILCAT_ALLOW_NONE]));
        assert_eq!(
            served_allow_list(&keys(&[""]), false),
            keys(&[TAILCAT_ALLOW_NONE])
        );
        assert_eq!(
            served_allow_list(&keys(&["key-a"]), false),
            keys(&["key-a"])
        );
        assert!(served_allow_list(&keys(&["key-a"]), true).is_empty());
        assert!(served_allow_list(&[], true).is_empty());
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
    fn rows_paired_before_11_0_1_become_legacy_and_lose_their_token() {
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
        .unwrap();
        db.execute_batch(include_str!(
            "../storage/migrations/0059_paired_device_api_token.sql"
        ))
        .unwrap();
        db.execute(
            "INSERT INTO remote_paired_devices (id, name, platform, sshKeyFingerprint, pairedAt, apiTokenHash)
             VALUES ('old', 'iPhone', 'ios', 'SHA256:x', '2026-10-01', 'hash')",
            [],
        )
        .unwrap();
        // The 11.0.1 notice reads names before 0062 runs; they must read on the old schema.
        assert_eq!(
            RemotePairingRepository::new(&db)
                .list_device_names()
                .unwrap(),
            vec!["iPhone".to_string()]
        );
        db.execute_batch(include_str!(
            "../storage/migrations/0062_pairing_v2_device_keys.sql"
        ))
        .unwrap();
        let row = RemotePairingRepository::new(&db)
            .find_device("old")
            .unwrap()
            .unwrap();
        assert_eq!(row.status, PairedDeviceStatus::Legacy);
        assert_eq!(row.api_token_hash, None);
        assert!(!row.legacy_key_line_removed);
        let tokens = DeviceApiTokens::default();
        reload_device_api_tokens(&db, &tokens).unwrap();
        assert!(!tokens.accepts("gxd_anything"));
    }

    #[test]
    fn a_device_without_a_key_never_counts_as_using_one() {
        let db = database();
        pair(&db, "legacy", None);
        assert!(!tailcat_client_key_in_use(&db, "key-a").unwrap());
    }
}
