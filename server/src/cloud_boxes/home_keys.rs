//! This computer's own Easy Connect keys for its own Cloud Boxes.
//!
//! CDXC:CloudBoxes 2026-11-21 DECISION:
//! User (Q4356 "A but pls fix this for users not just for me"): the computer that makes a Cloud
//! Box can open chats on its own boxes. A box lets in only the tailcat client keys its boot ticket
//! names, and the ticket named the paired devices only, so the desktop's forward and `ghostex cloud
//! attach`, which dialed with tailcat's shared `client-default`, were refused by every box (seen on
//! the first real Instabox boot of GitHub #698). This computer now keeps keys made only for its
//! boxes, one per purpose like its keys for paired computers (`app` for the desktop's forward,
//! `server` for the CLI's attach; one key shared by both starved one of them, GitHub #526), and
//! every ticket it writes (create, wake) and the phone's kit name their public halves next to the
//! devices. A box reads its device list only from its ticket at each boot (`enableForBoxDevices`,
//! and the removal-only pruning of saved devices when its gxserver starts), so a box woken from a
//! snapshot, or by the phone, gets the keys the same way, on every provider that reaches boxes over
//! Easy Connect. The paired devices' list and its rules are unchanged, `client-default` never joins
//! a ticket, and the private halves never leave this computer. The keys are used only to dial an
//! address that is one of this computer's own boxes, and they go when its last box is deleted (the
//! next box makes new ones). Each purpose has a few keys, one per open box (`SLOTS`).
//! SEE-ALSO: server/src/remote_access/computer_link_keys.rs (`easy_connect_link_key_answer`, which
//! names them to the desktop and the CLI), server/src/cloud_boxes/boot_steps.rs
//! (`allowed_device_keys`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderId};

use super::store;
use crate::paths::GxserverPaths;
use crate::remote_access::computer_link_keys::{
    LINK_KEY_PURPOSE_APP, LINK_KEY_PURPOSE_SERVER, make_client_key,
};

const PURPOSES: [&str; 2] = [LINK_KEY_PURPOSE_APP, LINK_KEY_PURPOSE_SERVER];

/// How many keys of each purpose this computer keeps for its boxes.
///
/// CDXC:CloudBoxes 2026-11-21 FIX: review of c1d1a8a3a (finding 1): with one key per purpose, two
/// boxes open at once dialed with the same key, and a relay delivers a key's traffic only to that
/// key's newest connection (GitHub #526): 8 of 56 relayed pings went unanswered, none on separate
/// keys. A box can't be given a key once it is running (its devices only ever go), so every ticket
/// names a small fixed set, and each open box dials with a slot of its own (`slot_for`): the first
/// open box the first key of each purpose, the next the second, and so on. Only a fifth box open at
/// once shares a slot again.
const SLOTS: usize = 4;

fn keys_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

fn key_file(paths: &GxserverPaths, purpose: &str, slot: usize) -> PathBuf {
    paths
        .root_dir
        .join("pairing")
        .join("link-keys")
        .join(format!("own-cloud-boxes-{purpose}-{slot}.json"))
}

/// The public halves of all of this computer's box keys, made the first time. None without an
/// Easy Connect helper here (nothing on this computer could dial a box then).
pub(super) fn public_keys(paths: &GxserverPaths) -> Vec<String> {
    let Some(binary) = crate::tailcat::resolve_client_tailcat_binary() else {
        return Vec::new();
    };
    let _guard = keys_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    PURPOSES
        .iter()
        .flat_map(|purpose| (0..SLOTS).map(move |slot| (*purpose, slot)))
        .filter_map(|(purpose, slot)| {
            let file = key_file(paths, purpose, slot);
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            make_client_key(&binary, &file)
                .map_err(|error| {
                    eprintln!("[gxserver] Cloud Boxes: no {purpose} key {slot} for this computer's boxes ({error:#})");
                })
                .ok()
        })
        .collect()
}

/// Which open box holds which slot, in this gxserver.
fn slots() -> &'static Mutex<HashMap<String, usize>> {
    static SLOTS_HELD: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    SLOTS_HELD.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The slot `box_id` dials with: the one it holds, else the lowest no open box holds. A box gives
/// its slot back once it is no longer among `open` (running, idle or being dialed now). `held` is
/// the locked table, taken before the rows were read so two asks can't both take one slot.
fn slot_for(held: &mut HashMap<String, usize>, box_id: &str, open: &[String]) -> usize {
    held.retain(|holder, _| holder == box_id || open.iter().any(|id| id == holder));
    if let Some(slot) = held.get(box_id) {
        return *slot;
    }
    let slot = (0..SLOTS)
        .find(|slot| !held.values().any(|taken| taken == slot))
        .unwrap_or(held.len() % SLOTS);
    held.insert(box_id.to_string(), slot);
    slot
}

/// The key file `purpose` dials `address` with when `address` is one of this computer's own boxes
/// (not deleted, reached over Easy Connect): that box's slot of `purpose`. `None` for any other
/// address.
pub(crate) fn own_box_key(paths: &GxserverPaths, address: &str, purpose: &str) -> Option<PathBuf> {
    let address = address.trim();
    if !PURPOSES.contains(&purpose) || address.is_empty() {
        return None;
    }
    // Before the rows are read (review of eaa2dfdb1): an ask that read them earlier could otherwise
    // give back a slot another ask had just taken for a box it hadn't seen yet.
    let mut held = slots()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let rows: Vec<_> = store::list_boxes(paths)
        .ok()?
        .into_iter()
        .map(|stored| stored.row)
        .filter(|row| row.state != CloudBoxState::Deleted && row.provider != CloudProviderId::Local)
        .collect();
    let row = rows
        .iter()
        .find(|row| row.tailcat_address.as_deref().map(str::trim) == Some(address))?;
    let open: Vec<String> = rows
        .iter()
        .filter(|other| other.state.is_reachable() || other.box_id == row.box_id)
        .map(|other| other.box_id.clone())
        .collect();
    let file = key_file(paths, purpose, slot_for(&mut held, &row.box_id, &open));
    file.is_file().then_some(file)
}

/// After the user deleted a box: once none of this computer's boxes is left, its box keys go.
///
/// CDXC:CloudBoxes 2026-11-21 FIX: review of c1d1a8a3a (finding 7): reconcile also called this when
/// a provider's list no longer showed boxes, and one empty list (a provider hiccup) marked every
/// box deleted and deleted the keys; when the rows came back, this computer couldn't open those
/// boxes until they were stopped and woken. Only the user's Delete (`lifecycle::delete`) calls it.
pub(super) fn forget_when_no_box_is_left(paths: &GxserverPaths) {
    let _guard = keys_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Ok(boxes) = store::list_boxes(paths) else {
        return;
    };
    if boxes
        .iter()
        .any(|stored| stored.row.state != CloudBoxState::Deleted)
    {
        return;
    }
    for purpose in PURPOSES {
        for slot in 0..SLOTS {
            let _ = std::fs::remove_file(key_file(paths, purpose, slot));
        }
    }
}
