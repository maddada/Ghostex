//! A connected computer removed from this app and added again: the new entry has a new machine id,
//! so a new owner key, which that computer never let in, while the old entry's key, which it did,
//! sat unused and its server-id pin locked the new entry out.

/*
CDXC:Security 2026-11-20 FIX:
The user removed M7-Desktop from the Mac and added it again (17 Nov, now through Easy Connect), and from then on the Mac could approve nothing there: pasting M7-Desktop's code on m7-arch showed only "M7-Desktop asks for your approval … Approve it in Ghostex on …" with no Approve anywhere the user looked. Each connected computer's owner key is named after this app's machine id for it (review round 2, R2-1 in `owner_key_for`), so the new entry had a key M7-Desktop never enrolled, and the old entry's pin (`owner-session-pins.json`, its machine id → S8g) refused the new entry's signature even once that key would be let in ("that server id is another machine's"). The user (11.12.0): "When a computer is re-added with the same server id, the Mac keeps approving: carry the old entry's enrolled approver key over (or let the old key sign) … nothing auto-approves". So (1) a pin whose machine is no longer in Settings > Remote blocks nobody, and goes when another entry pins its server id (the pins keep two configured computers that share one of the 360 ids apart, which a removed one no longer is); (2) the old entry's key, where that computer still lists it, is one more old key for the unlock card ("Let this Mac manage <computer> again?", `old_key_unlock_offer`): only on the user's Approve of that card, which names the computer and what it says it is, it signs that one waiting request for the new key (`/api/approveWithOwnerKey`, never the legacy door session), under every check the old shared key has (a full registration pass, no other connected machine with that id, Touch ID where this Mac asks for it). The new key is let in door-only and the old one retired there.
*/

use std::collections::HashSet;

use serde_json::Value;

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

/// Whether `machine_id`'s pin is left from an entry no longer in Settings > Remote.
pub(super) fn is_removed(machine_id: &str, configured: Option<&HashSet<String>>) -> bool {
    configured.is_some_and(|ids| !ids.contains(machine_id))
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

/// The owner keys of removed entries that pinned `server_id` (never `machine_id`'s own), as kept
/// in the key store; none is made here.
pub(super) fn removed_entry_keys(
    machine_id: &str,
    server_id: &str,
) -> Vec<ghostex_owner_keys::DeviceKey> {
    let configured = configured_machine_ids();
    let pins: std::collections::BTreeMap<String, String> =
        std::fs::read_to_string(super::pins_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
    pins.iter()
        .filter(|(machine, pinned)| {
            machine.as_str() != machine_id
                && pinned.as_str() == server_id
                && is_removed(machine, configured.as_ref())
        })
        .filter_map(|(machine, _)| scoped_key_name(machine).ok())
        .filter_map(|name| {
            ghostex_owner_keys::load_scoped_owner_key(&name)
                .ok()
                .flatten()
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
/// entries' for `server_id`, and the old shared one. That computer leaves them out of where to
/// approve (`exceptKeyIds`, CDXC:Security 2026-11-20 at `approvers_here` in
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
        keys.extend(removed_entry_keys(machine_id, server_id));
    }
    if let Ok(shared) = ghostex_owner_keys::load_or_create_owner_key() {
        keys.push(shared);
    }
    keys.iter()
        .filter_map(|key| super::key_id_of(&key.public_key_b64()))
        .collect()
}
