//! The Remove button on the bell row that says a phone was added for Easy Connect without a click.
//!
//! CDXC:RemotePairing 2026-11-12 DECISION:
//! User (via the coordinator): "remove button: yes", the notice for a phone gxserver added for Easy
//! Connect by itself (the owner's own Tailscale device, remote_access/tailnet_owner.rs) carries a
//! Remove, on the desktop's bell and the phone's notification card. The row says which device it
//! is (`removeDevice`) only while that device is still paired here, so a Remove done elsewhere
//! (Settings › Remote, another client) takes the button away with the device; clients call
//! `/api/removePairedDevice` with its id. Only a pairing nobody approved gets it: one the owner
//! approved with a click was their own decision.
//! SEE-ALSO: apps/desktop/src/app/window/titlebar_notifications_panel.rs, the phone's
//! src/notifications/ in the phone repo.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::audit::StoredEntry;
use crate::remote_access::{PairedDeviceStatus, RemotePairingRepository};

/// The device a security log entry added without a click: a `pairing.pair` that ended `done`
/// (an approved one ends `approved`), done by the paired device it names.
fn device_added_without_a_click(entry: &StoredEntry) -> Option<&str> {
    if entry.action != "pairing.pair"
        || entry.outcome != "done"
        || entry.actor.kind != "pairedDevice"
    {
        return None;
    }
    entry.actor.id.as_deref().filter(|id| !id.is_empty())
}

/// Adds `removeDevice: { id, name }` to each security row in `items` whose entry (from `entries`)
/// added a device without a click that is still paired.
pub(crate) fn overlay_removable_devices_from(
    db: &Connection,
    items: &mut [Value],
    entries: &[StoredEntry],
) {
    let repository = RemotePairingRepository::new(db);
    for item in items.iter_mut() {
        let Some(seq) = item["securityEntry"].as_u64() else {
            continue;
        };
        let Some(device_id) = entries
            .iter()
            .find(|entry| entry.seq == seq)
            .and_then(device_added_without_a_click)
        else {
            continue;
        };
        let Ok(Some(device)) = repository.find_device(device_id) else {
            continue;
        };
        if device.status == PairedDeviceStatus::Paired {
            item["removeDevice"] = json!({ "id": device.id, "name": device.name });
        }
    }
}

/// [`overlay_removable_devices_from`] with the security log's entries back to the oldest row's.
pub(crate) fn overlay_removable_devices(db: &Connection, items: &mut [Value]) {
    let Some(oldest) = items
        .iter()
        .filter_map(|item| item["securityEntry"].as_u64())
        .min()
    else {
        return;
    };
    let entries = crate::audit::live_log().read_back_to(oldest, 0);
    overlay_removable_devices_from(db, items, &entries);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_access::{now_iso, RemotePairedDeviceRecord};

    fn entry(
        seq: u64,
        action: &str,
        outcome: &str,
        actor_kind: &str,
        actor_id: Option<&str>,
    ) -> StoredEntry {
        serde_json::from_value(json!({
            "v": 1,
            "seq": seq,
            "atMs": 1,
            "action": action,
            "summary": "s",
            "actor": { "kind": actor_kind, "label": "Phone Test", "id": actor_id },
            "outcome": outcome,
            "prevHash": "",
        }))
        .expect("entry")
    }

    fn device(id: &str, status: PairedDeviceStatus) -> RemotePairedDeviceRecord {
        RemotePairedDeviceRecord {
            id: id.to_string(),
            name: format!("Phone {id}"),
            platform: "ios".to_string(),
            ssh_key_fingerprint: String::new(),
            tailcat_client_key: None,
            api_token_hash: None,
            paired_at: now_iso(),
            last_seen_at: None,
            status,
            device_key: None,
            api_token_expires_at: None,
            approver_key_id: None,
            legacy_key_line_removed: false,
        }
    }

    #[test]
    fn only_a_still_paired_device_added_without_a_click_gets_remove() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage");
        let db = crate::storage::open_gxserver_database(&paths).expect("db");
        let repository = RemotePairingRepository::new(&db);
        repository
            .insert_device(&device("auto", PairedDeviceStatus::Paired))
            .unwrap();
        repository
            .insert_device(&device("clicked", PairedDeviceStatus::Paired))
            .unwrap();
        let entries = vec![
            entry(1, "pairing.pair", "done", "pairedDevice", Some("auto")),
            entry(
                2,
                "pairing.pair",
                "approved",
                "pairedDevice",
                Some("clicked"),
            ),
            entry(3, "pairing.pair", "done", "pairedDevice", Some("gone")),
            entry(4, "collabEnable", "done", "pairedDevice", Some("auto")),
        ];
        let mut items: Vec<Value> = (1..=5)
            .map(|seq| json!({ "id": format!("n{seq}"), "securityEntry": seq }))
            .chain([json!({ "id": "session-row" })])
            .collect();
        overlay_removable_devices_from(&db, &mut items, &entries);
        assert_eq!(
            items[0]["removeDevice"],
            json!({ "id": "auto", "name": "Phone auto" })
        );
        // Approved with a click, removed already, another action, no entry, a session row: none.
        for item in &items[1..] {
            assert!(item.get("removeDevice").is_none(), "{item}");
        }
    }
}
