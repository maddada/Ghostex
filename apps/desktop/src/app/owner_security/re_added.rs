//! A connected computer removed from this app and added again: the new entry has a new machine id,
//! so a new owner key, which that computer never let in, while the old entry's key, which it did,
//! sat unused and its server-id pin locked the new entry out.

/*
CDXC:Security 2026-11-20 FIX:
The user removed M7-Desktop from the Mac and added it again (17 Nov, now through Easy Connect), and from then on the Mac could approve nothing there: pasting M7-Desktop's code on m7-arch showed only "M7-Desktop asks for your approval … Approve it in Ghostex on …" with no Approve anywhere the user looked. Each connected computer's owner key is named after this app's machine id for it (review round 2, R2-1 in `owner_key_for`), so the new entry had a key M7-Desktop never enrolled, and the old entry's pin (`owner-session-pins.json`, its machine id → S8g) refused the new entry's signature even once that key would be let in ("that server id is another machine's"). The user (11.12.0): "When a computer is re-added with the same server id, the Mac keeps approving: carry the old entry's enrolled approver key over (or let the old key sign) … nothing auto-approves". So a removed entry's pin and key go to an entry of the same computer: (1) its pin blocks that entry no more, and goes when that entry pins the server id; (2) its key, where that computer still lists it, is one more old key for the unlock card (`old_key_unlock_offer`): only on the user's Approve of that card it signs that one waiting request for the new key (`/api/approveWithOwnerKey`, never the legacy door session), under every check the old shared key has (a full registration pass, no other connected machine with that id, Touch ID where this Mac asks for it). The new key is let in door-only and the old one retired there.
Fable 5.2's review (finding 1, the user's 1A): any entry not pinned yet could answer with a removed entry's id and get its pin and key, so an agent on a connected computer B holding A's token file could make B answer as A, and one click later this app's door sessions for A went through B. An entry now takes a removed entry's pin and key only when the server id it first registered with (kept by this app, `computer-first-server-ids.json`, from its own registration) is that id: B, which first registered as itself, is refused when it answers as A. The removed entry's name is kept with it, and the card says from this app's own record that this Mac managed that server before as that name, instead of "was added again", which B's swap would have read as. Finding 2: only Settings > Remote entries (`remote-` ids) count as removed, never a Cloud Box (`cloudbox:`), which that list never holds.
*/

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What this app's id for a Settings > Remote entry starts with.
const REMOTE_ENTRY_PREFIX: &str = "remote-";

/// The server id an entry first registered with, and its name in Settings > Remote (kept current
/// while it answers with that id, and kept once the entry is removed).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct FirstRegistration {
    server_id: String,
    name: String,
}

static FIRST_IDS: Mutex<Option<BTreeMap<String, FirstRegistration>>> = Mutex::new(None);

fn first_ids_path() -> std::path::PathBuf {
    crate::shared_settings::ghostex_storage_paths()
        .state_dir
        .join("computer-first-server-ids.json")
}

fn with_first_ids<R>(run: impl FnOnce(&mut BTreeMap<String, FirstRegistration>) -> R) -> R {
    let mut cache = FIRST_IDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let ids = cache.get_or_insert_with(|| {
        std::fs::read_to_string(first_ids_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    });
    run(ids)
}

/// A connected computer's registration learned `server_id` for entry `machine_id`, named `name`:
/// the first id is kept for good, and the name while it answers with that id.
pub(crate) fn note_registered(machine_id: &str, server_id: &str, name: &str) {
    if !machine_id.starts_with(REMOTE_ENTRY_PREFIX) || server_id.is_empty() {
        return;
    }
    let changed = with_first_ids(|ids| match ids.get_mut(machine_id) {
        None => {
            ids.insert(
                machine_id.to_string(),
                FirstRegistration {
                    server_id: server_id.to_string(),
                    name: name.to_string(),
                },
            );
            Some(ids.clone())
        }
        Some(first) if first.server_id == server_id && first.name != name => {
            first.name = name.to_string();
            Some(ids.clone())
        }
        Some(_) => None,
    });
    if let Some(ids) = changed {
        let path = first_ids_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(&ids) {
            let _ = std::fs::write(path, text);
        }
    }
}

fn first_registration(machine_id: &str) -> Option<FirstRegistration> {
    with_first_ids(|ids| ids.get(machine_id).cloned())
}

/// The machine ids in Settings > Remote now; `None` when the settings say nothing about remote
/// machines (not read yet), so nothing counts as removed.
pub(super) fn configured_machine_ids() -> Option<HashSet<String>> {
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let machines = snapshot.object().get("remoteMachines")?.as_array()?.clone();
    Some(
        machines
            .iter()
            .filter_map(crate::app::helpers::gpui_remote_machine_id_from_value)
            .collect(),
    )
}

/// Whether `machine_id` is a Settings > Remote entry that is no longer there (never a Cloud Box).
fn is_removed(machine_id: &str, configured: Option<&HashSet<String>>) -> bool {
    machine_id.starts_with(REMOTE_ENTRY_PREFIX)
        && configured.is_some_and(|ids| !ids.contains(machine_id))
}

/// Whether entry `machine_id`, which first registered with `first_server_id`, may take over removed
/// entry `removed`'s pin and key for `server_id`: `removed` is gone from Settings > Remote, and
/// `machine_id` first registered with `server_id`.
pub(super) fn takes_over_with(
    first_server_id: Option<&str>,
    machine_id: &str,
    server_id: &str,
    removed: &str,
    configured: Option<&HashSet<String>>,
) -> bool {
    removed != machine_id && is_removed(removed, configured) && first_server_id == Some(server_id)
}

/// [`takes_over_with`] with the id `machine_id` first registered with, as this app kept it.
pub(super) fn takes_over(
    machine_id: &str,
    server_id: &str,
    removed: &str,
    configured: Option<&HashSet<String>>,
) -> bool {
    let first = first_registration(machine_id).map(|first| first.server_id);
    takes_over_with(first.as_deref(), machine_id, server_id, removed, configured)
}

/// The pins' answer for entry `machine_id` answering `server_id`, which first registered with
/// `first_server_id`: whether this app may sign for it (`check_server_id`, before the check of the
/// other connected machines). Its own pin must match; another entry's pin of that id refuses it
/// unless it may take that one over ([`takes_over_with`]).
pub(super) fn pins_allow(
    pins: &BTreeMap<String, String>,
    first_server_id: Option<&str>,
    machine_id: &str,
    server_id: &str,
    configured: Option<&HashSet<String>>,
) -> bool {
    if let Some(pinned) = pins.get(machine_id) {
        return pinned == server_id;
    }
    !pins.iter().any(|(other, pinned)| {
        pinned == server_id
            && !takes_over_with(first_server_id, machine_id, server_id, other, configured)
    })
}

/// [`pins_allow`] with the id `machine_id` first registered with, as this app kept it.
pub(super) fn pins_allow_for(
    pins: &BTreeMap<String, String>,
    machine_id: &str,
    server_id: &str,
    configured: Option<&HashSet<String>>,
) -> bool {
    let first = first_registration(machine_id).map(|first| first.server_id);
    pins_allow(pins, first.as_deref(), machine_id, server_id, configured)
}

/// The key-store name of this app's owner key for machine `machine_id` (review round 2, R2-1:
/// this app's own id for it mixed with the local salt).
pub(super) fn scoped_key_name(machine_id: &str) -> Result<String, String> {
    let salt = ghostex_owner_keys::local_salt().map_err(|error| error.to_string())?;
    let mut input = b"ghostex-remote-owner-key\n".to_vec();
    input.extend_from_slice(&salt);
    input.push(b'\n');
    input.extend_from_slice(machine_id.as_bytes());
    let digest = ring::digest::digest(&ring::digest::SHA256, &input);
    Ok(format!("remote-{}", hex::encode(&digest.as_ref()[..8])))
}

fn pins() -> BTreeMap<String, String> {
    std::fs::read_to_string(super::pins_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Whether this app signed for entry `machine_id` before (it has a server-id pin).
pub(super) fn is_pinned(machine_id: &str) -> bool {
    pins().contains_key(machine_id)
}

/// A removed entry's owner key that entry `machine_id` may use ([`takes_over`]), and the name this
/// app had for that entry ("" when it never kept one).
pub(super) struct RemovedEntry {
    pub(super) key: ghostex_owner_keys::DeviceKey,
    pub(super) name: String,
}

/// The owner keys of removed entries that pinned `server_id` and that `machine_id` may take over,
/// as kept in the key store; none is made here.
pub(super) fn removed_entry_keys(machine_id: &str, server_id: &str) -> Vec<RemovedEntry> {
    let configured = configured_machine_ids();
    pins()
        .into_iter()
        .filter(|(removed, pinned)| {
            pinned == server_id && takes_over(machine_id, server_id, removed, configured.as_ref())
        })
        .filter_map(|(removed, _)| {
            let key = ghostex_owner_keys::load_scoped_owner_key(&scoped_key_name(&removed).ok()?)
                .ok()
                .flatten()?;
            let name = first_registration(&removed)
                .map(|first| first.name)
                .unwrap_or_default();
            Some(RemovedEntry { key, name })
        })
        .collect()
}

/// Whether `status` (that computer's `/api/securityStatus`) lists `key` as an owner key.
pub(super) fn lists_key(status: &Value, key: &ghostex_owner_keys::DeviceKey) -> bool {
    let Some(key_id) = super::key_id_of(&key.public_key_b64()) else {
        return false;
    };
    status
        .get("ownerKeys")
        .and_then(Value::as_array)
        .is_some_and(|keys| {
            keys.iter()
                .any(|listed| super::text(listed, "keyId").as_deref() == Some(key_id.as_str()))
        })
}

/// The key ids of every owner key this app holds for `machine_id`'s computer: its own, the removed
/// entries' it may take over for `server_id`, and the old shared one. That computer leaves them out
/// of where to approve (`exceptKeyIds`, CDXC:Security 2026-11-20 at `approvers_here` in
/// server/src/server/security_http.rs), since this app is the one asking.
pub(super) fn own_key_ids(machine_id: &str, server_id: Option<&str>) -> Vec<String> {
    let mut keys: Vec<ghostex_owner_keys::DeviceKey> = Vec::new();
    if let Some(own) = scoped_key_name(machine_id).ok().and_then(|name| {
        ghostex_owner_keys::load_scoped_owner_key(&name)
            .ok()
            .flatten()
    }) {
        keys.push(own);
    }
    if let Some(server_id) = server_id {
        keys.extend(
            removed_entry_keys(machine_id, server_id)
                .into_iter()
                .map(|removed| removed.key),
        );
    }
    if let Ok(shared) = ghostex_owner_keys::load_or_create_owner_key() {
        keys.push(shared);
    }
    keys.iter()
        .filter_map(|key| super::key_id_of(&key.public_key_b64()))
        .collect()
}
