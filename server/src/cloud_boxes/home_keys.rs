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
//! next box makes new ones).
//! SEE-ALSO: server/src/remote_access/computer_link_keys.rs (`easy_connect_link_key_answer`, which
//! names them to the desktop and the CLI), server/src/cloud_boxes/boot_steps.rs
//! (`allowed_device_keys`).

use std::path::PathBuf;
use std::sync::Mutex;

use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderId};

use super::store;
use crate::paths::GxserverPaths;
use crate::remote_access::computer_link_keys::{
    make_client_key, LINK_KEY_PURPOSE_APP, LINK_KEY_PURPOSE_SERVER,
};

const PURPOSES: [&str; 2] = [LINK_KEY_PURPOSE_APP, LINK_KEY_PURPOSE_SERVER];

fn keys_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

fn key_file(paths: &GxserverPaths, purpose: &str) -> PathBuf {
    paths
        .root_dir
        .join("pairing")
        .join("link-keys")
        .join(format!("own-cloud-boxes-{purpose}.json"))
}

/// The public halves of this computer's box keys, made the first time. None without an Easy
/// Connect helper here (nothing on this computer could dial a box then).
pub(super) fn public_keys(paths: &GxserverPaths) -> Vec<String> {
    let Some(binary) = crate::tailcat::resolve_client_tailcat_binary() else {
        return Vec::new();
    };
    let _guard = keys_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    PURPOSES
        .iter()
        .filter_map(|purpose| {
            let file = key_file(paths, purpose);
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            make_client_key(&binary, &file)
                .map_err(|error| {
                    eprintln!("[gxserver] Cloud Boxes: no {purpose} key for this computer's boxes ({error:#})");
                })
                .ok()
        })
        .collect()
}

/// The key file `purpose` dials `address` with when `address` is one of this computer's own boxes
/// (not deleted, reached over Easy Connect); `None` for any other address.
pub(crate) fn own_box_key(paths: &GxserverPaths, address: &str, purpose: &str) -> Option<PathBuf> {
    let address = address.trim();
    if !PURPOSES.contains(&purpose) || address.is_empty() {
        return None;
    }
    let file = key_file(paths, purpose);
    if !file.is_file() {
        return None;
    }
    let own = store::list_boxes(paths).ok()?.into_iter().any(|stored| {
        stored.row.state != CloudBoxState::Deleted
            && stored.row.provider != CloudProviderId::Local
            && stored.row.tailcat_address.as_deref().map(str::trim) == Some(address)
    });
    own.then_some(file)
}

/// After a box is deleted: once none of this computer's boxes is left, its box keys go.
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
        let _ = std::fs::remove_file(key_file(paths, purpose));
    }
}
