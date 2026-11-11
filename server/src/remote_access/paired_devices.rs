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
        // Paired through its own SSH login (`/api/pairOverSsh`): a token, but neither an SSH key
        // line nor an Easy Connect key.
        "overSsh": device.api_token_hash.is_some()
            && device.ssh_key_fingerprint.is_empty()
            && device.tailcat_client_key.is_none(),
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
    let mut removed = false;
    for key_id in key_ids {
        if let Some(key) = security.approver_keys.remove(&key_id) {
            removed = true;
            security.publish(SecurityEvent::ApproverKeyRemoved {
                key,
                by: by.clone(),
            });
        }
    }
    if removed {
        let _ = security.save();
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
    // A computer paired through Easy Connect has no SSH key line (its fingerprint is empty), and
    // looking for one could show a Windows administrator prompt for nothing.
    let has_key_line = !device.ssh_key_fingerprint.is_empty();
    let remove_key = |id: &str| {
        if has_key_line {
            remove_key(id)
        } else {
            Ok(false)
        }
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
    let link_key = repository.remove_device_link_key(&device.id)?;
    let purpose_keys = repository.remove_device_purpose_keys(&device.id)?;
    if let Some(client_key) = device.tailcat_client_key.as_deref() {
        remove_tailcat_client_key(paths, db, tailcat_runtime, client_key)?;
    }
    if let Some(link_key) = link_key.as_deref() {
        remove_tailcat_client_key(paths, db, tailcat_runtime, link_key)?;
    }
    for (_, purpose_key) in purpose_keys {
        remove_tailcat_client_key(paths, db, tailcat_runtime, &purpose_key)?;
    }
    Ok(Some(device))
}

/*
CDXC:RemotePairing 2026-11-05 WHY:
A phone that pairs again replaces its earlier entries, so it is listed once (desktop Settings and the phone). The phone is recognized by its tailcat client key: it lives in the app's private storage, outside backups, and stays the same across pairings, while the SSH key is generated anew each time and the device name ("iPhone") is shared by many phones. A phone without a client key (an app build from before Easy Connect allow-listing) cannot be told apart from another phone and is never merged. Each earlier entry is revoked like Remove does it, key line first and then the row (its API token goes with it), but the tailcat key stays on the allow-list because the new entry uses it. Every earlier entry with the key goes, so a list that already holds duplicates collapses to one on the phone's next pairing. A key line that cannot be removed (a declined Windows administrator prompt) keeps that entry listed so Remove can retry it; the new pairing still succeeds. Runs after the new entry is saved, so a failure here never costs the phone its new access. Returns the replaced entries.

CDXC:RemotePairing 2026-11-11 SECURITY:
Review finding 6: the tailcat client key is only what the request STATES. Nothing checks it against the tunnel the request came in on (tailcat serve hands gxserver a plain local connection and reports no peer for it), so a device that held a code and was approved could name another paired device's key and retire that device's pairing (its SSH key line, token, push registrations and row), and take over the link and purpose keys it had registered. An earlier entry is now retired only when it is proven to be the same device: its device key is the key that signed this pairing's status ask (`pairing_status` verifies that signature before it finishes a pairing), and a phone or computer keeps one device key for good (Secure Enclave / Android Keystore, `computer_pairing::load_or_create_device_key`). A pre-11.0.1 `legacy` entry has no device key to compare; it is retired as before only when it holds no access any more (its unrestricted key line is gone, `retire_legacy_pairings`), so a false claim can cost it nothing but its "pair again" line. A request that names a key another paired device holds under another device key is refused (`tailcat_key_held_by_another_device`, `begin_pairing`). Approval is unchanged.
*/
pub fn replace_earlier_pairings_of_same_phone(
    paths: &GxserverPaths,
    db: &Connection,
    new_device_id: &str,
    tailcat_client_key: Option<&str>,
    device_public_key: &str,
    mut remove_key_line: impl FnMut(&str) -> Result<bool>,
    mut on_kept: impl FnMut(&RemotePairedDeviceRecord, &anyhow::Error),
) -> Result<Vec<RemotePairedDeviceRecord>> {
    let Some(client_key) = tailcat_client_key else {
        return Ok(Vec::new());
    };
    let repository = RemotePairingRepository::new(db);
    let mut replaced = Vec::new();
    for device in repository.list_devices()? {
        if device.id == new_device_id
            || device.tailcat_client_key.as_deref() != Some(client_key)
            || !is_earlier_pairing_of(&device, device_public_key)
        {
            continue;
        }
        let removed = if device.ssh_key_fingerprint.is_empty() {
            Ok(false)
        } else {
            remove_key_line(&device.id)
        };
        if let Err(error) = removed {
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
        // The phone keeps dialing this computer with the link key it registered
        // (`register_device_link_key`), so its new entry takes it over instead of losing it.
        if let Some(link_key) = repository.remove_device_link_key(&device.id)? {
            if repository.device_link_key(new_device_id)?.is_none() {
                repository.set_device_link_key(new_device_id, &link_key)?;
            }
        }
        // A computer paired again keeps dialing with the keys it registered per purpose, too.
        for (purpose, key) in repository.remove_device_purpose_keys(&device.id)? {
            if repository
                .device_purpose_key(new_device_id, &purpose)?
                .is_none()
            {
                repository.set_device_purpose_key(new_device_id, &purpose, &key)?;
            }
        }
        if let Some(removed) = repository.remove_device(&device.id)? {
            replaced.push(removed);
        }
    }
    Ok(replaced)
}

/// Whether `device`, an entry with the same tailcat key, is an earlier pairing of the device whose
/// key is `device_public_key` (see `replace_earlier_pairings_of_same_phone`).
fn is_earlier_pairing_of(device: &RemotePairedDeviceRecord, device_public_key: &str) -> bool {
    match device.device_key.as_ref() {
        Some(key) => key.public_key == device_public_key,
        None => {
            device.status == PairedDeviceStatus::Legacy
                && (device.legacy_key_line_removed || device.ssh_key_fingerprint.is_empty())
        }
    }
}

/// A paired entry that holds `tailcat_client_key` under a device key other than
/// `device_public_key`: a request naming that key is not that device, and is refused
/// (`replace_earlier_pairings_of_same_phone`). A `legacy` entry never is one.
pub fn tailcat_key_held_by_another_device(
    db: &Connection,
    tailcat_client_key: &str,
    device_public_key: &str,
) -> Result<Option<RemotePairedDeviceRecord>> {
    Ok(RemotePairingRepository::new(db)
        .list_devices()?
        .into_iter()
        .find(|device| {
            device.status == PairedDeviceStatus::Paired
                && device.tailcat_client_key.as_deref() == Some(tailcat_client_key)
                && device
                    .device_key
                    .as_ref()
                    .is_none_or(|key| key.public_key != device_public_key)
        }))
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

/// Where a paired device registers its own Easy Connect key for this computer.
pub const EASY_CONNECT_CLIENT_KEY_ENDPOINT: &str = "/api/registerEasyConnectClientKey";

/// Where a paired computer registers its keys per purpose, signed with its pairing device key.
pub const COMPUTER_EASY_CONNECT_KEYS_ENDPOINT: &str = "/api/registerComputerEasyConnectKeys";

/// Why a link key was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkKeyError {
    /// Not a `nodekey:` public key.
    BadKey,
    /// The caller is no paired (v2) device here.
    NotPaired,
}

/*
CDXC:RemotePairing 2026-11-10 WHY:
A phone or computer with several Easy Connect computers dialed all of them with its one tailcat
client key, and a relay delivers a key's traffic only to that key's newest connection, so each new
tunnel starved the others for 15 to 30 s (measured on Ghostex's relay, where every computer meets
its phones since 10.49.0). A paired device now registers a key of its own for this computer; it goes
on the allow-list beside the key it paired with, which stays valid (an older app keeps dialing with
it), and leaves with the device's row. Registering again replaces it. The device only ever names
its own row: the caller is the device token's own device.
SEE-ALSO: apps/mobile/app/src/machines/easyConnectLinkKey.ts, apps/mobile/tailcat-bridge/bridge.go (`UseMachineClientKeyFile`).
*/
/// Registers `client_key` as `device_id`'s link key for this computer. Returns whether anything
/// changed (the allow-list then restarts the Easy Connect helpers once).
pub fn register_device_link_key(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    device_id: &str,
    client_key: &str,
) -> Result<std::result::Result<bool, LinkKeyError>> {
    let client_key = client_key.trim();
    if !super::pair_device::is_tailcat_client_public_key(client_key) {
        return Ok(Err(LinkKeyError::BadKey));
    }
    let repository = RemotePairingRepository::new(db);
    let Some(device) = repository.find_device(device_id)? else {
        return Ok(Err(LinkKeyError::NotPaired));
    };
    if device.status != PairedDeviceStatus::Paired {
        return Ok(Err(LinkKeyError::NotPaired));
    }
    let previous = repository.device_link_key(device_id)?;
    let listed = read_tailcat_status_payload(paths, db, tailcat_runtime)
        .allowed_client_keys
        .iter()
        .any(|key| key == client_key);
    if previous.as_deref() == Some(client_key) && listed {
        return Ok(Ok(false));
    }
    repository.set_device_link_key(device_id, client_key)?;
    add_tailcat_client_key_when_allow_listed(paths, db, tailcat_runtime, client_key)?;
    if let Some(previous) = previous.filter(|previous| previous != client_key) {
        remove_tailcat_client_key(paths, db, tailcat_runtime, &previous)?;
    }
    Ok(Ok(true))
}

/// The purposes a paired computer registers a key of its own for: its desktop app's forward and its
/// gxserver's tunnels (CDXC:RemotePairing 2026-11-10 in `computer_link_keys.rs`).
pub const COMPUTER_KEY_PURPOSES: [&str; 2] = ["app", "server"];

/// Registers `keys` (`(purpose, key)`) for `device_id`, replacing the keys it had for those
/// purposes, with one allow-list change (one helper restart) for all of them. Returns whether
/// anything changed.
pub fn register_device_purpose_keys(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    device_id: &str,
    keys: &[(String, String)],
) -> Result<std::result::Result<bool, LinkKeyError>> {
    if keys.is_empty()
        || keys.iter().any(|(purpose, key)| {
            !COMPUTER_KEY_PURPOSES.contains(&purpose.as_str())
                || !super::pair_device::is_tailcat_client_public_key(key)
        })
    {
        return Ok(Err(LinkKeyError::BadKey));
    }
    let repository = RemotePairingRepository::new(db);
    let Some(device) = repository.find_device(device_id)? else {
        return Ok(Err(LinkKeyError::NotPaired));
    };
    if device.status != PairedDeviceStatus::Paired {
        return Ok(Err(LinkKeyError::NotPaired));
    }
    let mut added = Vec::new();
    let mut replaced = Vec::new();
    for (purpose, key) in keys {
        let previous = repository.device_purpose_key(device_id, purpose)?;
        repository.set_device_purpose_key(device_id, purpose, key)?;
        added.push(key.clone());
        if let Some(previous) = previous.filter(|previous| previous != key) {
            replaced.push(previous);
        }
    }
    let current = read_tailcat_status_payload(paths, db, tailcat_runtime).allowed_client_keys;
    if purpose_keys_allow_list(db, current.clone(), &added, &replaced)? == current {
        return Ok(Ok(false));
    }
    /*
    CDXC:RemotePairing 2026-11-10 WHY: the computer asks through its Easy Connect tunnel, and a new
    allow-list restarts the helper that tunnel runs on, so applied at once it cut off this request's
    own answer and the asking computer waited out its timeout before asking again. It is applied
    a moment after the answer leaves, against the list as it stands then.
    */
    let paths = paths.clone();
    let tailcat_runtime = tailcat_runtime.clone();
    std::thread::spawn(move || {
        std::thread::sleep(PURPOSE_KEYS_APPLY_AFTER);
        let applied = crate::storage::open_gxserver_database(&paths).and_then(|db| {
            let current =
                read_tailcat_status_payload(&paths, &db, &tailcat_runtime).allowed_client_keys;
            let next = purpose_keys_allow_list(&db, current.clone(), &added, &replaced)?;
            if next == current {
                return Ok(());
            }
            apply_tailcat_state_update(
                &paths,
                &db,
                &tailcat_runtime,
                TailcatStateUpdate::SetAllowedClientKeys {
                    allowed_client_keys: next,
                },
            )
            .map(|_| ())
        });
        if let Err(error) = applied {
            eprintln!("[gxserver] could not add a paired computer's Easy Connect keys: {error:#}");
        }
    });
    Ok(Ok(true))
}

/// `allowed` with `added` keys still registered put on and `replaced` keys no one uses any more
/// taken off.
fn purpose_keys_allow_list(
    db: &Connection,
    mut allowed: Vec<String>,
    added: &[String],
    replaced: &[String],
) -> Result<Vec<String>> {
    for key in added {
        if !tailcat_client_key_in_use(db, key)? {
            continue;
        }
        if let Some(next) = allow_list_with_client_key(allowed.clone(), key) {
            allowed = next;
        }
    }
    for key in replaced {
        if tailcat_client_key_in_use(db, key)? {
            continue;
        }
        if let Some(next) = allow_list_without_client_key(allowed.clone(), key) {
            allowed = next;
        }
    }
    Ok(allowed)
}

/// How long after answering a paired computer its new keys go on the allow-list.
const PURPOSE_KEYS_APPLY_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);

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
    let repository = RemotePairingRepository::new(db);
    Ok(repository
        .list_devices()?
        .iter()
        .any(|device| device.tailcat_client_key.as_deref() == Some(client_key))
        || repository.link_key_in_use(client_key)?)
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
        db.execute_batch(include_str!(
            "../storage/migrations/0072_paired_device_link_keys.sql"
        ))
        .expect("link keys table");
        db.execute_batch(include_str!(
            "../storage/migrations/0073_paired_device_purpose_keys.sql"
        ))
        .expect("purpose keys table");
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
