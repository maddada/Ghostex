//! An unpaired device leaves this computer's running Cloud Boxes at once, not at their next start.
//!
//! CDXC:CloudBoxes 2026-11-21 DECISION:
//! User (via the coordinator, after Q4356): "take an unpaired phone or computer off boxes that are
//! already running (push the updated device list to running boxes now, removal only)". A box reads
//! its devices from its boot ticket, so an unpaired device kept reaching a running box until that
//! box next started. Unpairing now sends the keys only that device held to every running box of
//! this computer, through the provider's exec like every other box step (no Easy Connect needed):
//! `ghostex box forget-devices` takes them out of the box's ticket and has its gxserver take every
//! saved device the ticket no longer names off Easy Connect (`pruneToBoxTicket`). Both steps only
//! ever remove, on the box itself, so nothing sent this way can let anyone in, and neither asks for
//! approval (taking a device off never does). A box that is starting when the device is unpaired
//! booted on a ticket written before, so the boot's last step takes off whatever that ticket names
//! and this computer no longer lists (`catch_up_after_boot`). A box that can't be reached now
//! (stopped, paused) gets a ticket without the device when it next starts, as before.
//! SEE-ALSO: server/src/remote_access/paired_devices.rs (`remove_paired_device_using`, the caller),
//! server/src/cloud_boxes/box_mode/boot.rs (`run_forget_devices`), server/src/tailcat/status.rs.

use ghostex_gx_protocol::cloud_box::{box_verbs, BoxBootTicket, CloudProviderId};

use super::boot_steps;
use super::ops::{Ops, Target};
use super::store;
use crate::paths::GxserverPaths;

fn forget_on_box(ops: &Ops, target: &Target, keys: &[String]) -> Result<(), String> {
    boot_steps::run_box_verb(
        ops,
        target,
        box_verbs::FORGET_DEVICES,
        &["--keys", &keys.join(",")],
        60,
    )
    .map(|_| ())
}

/// The running boxes of this computer reached over Easy Connect: provider, target and name.
fn running_boxes(paths: &GxserverPaths) -> Vec<(CloudProviderId, Target, String)> {
    let Ok(boxes) = store::list_boxes(paths) else {
        return Vec::new();
    };
    boxes
        .into_iter()
        .map(|stored| stored.row)
        .filter(|row| row.state.is_reachable() && row.provider != CloudProviderId::Local)
        .filter_map(|row| {
            Some((
                row.provider.clone(),
                Target {
                    box_id: row.box_id.clone(),
                    provider_box_id: row.provider_box_id.clone()?,
                },
                row.name,
            ))
        })
        .collect()
}

/// Takes `keys` (an unpaired device's, held by no device still paired) off every running box of
/// this computer that is reached over Easy Connect. Returns at once; the boxes are told in the
/// background, one after another.
pub(crate) fn take_off_running_boxes(paths: &GxserverPaths, keys: Vec<String>) {
    let keys: Vec<String> = keys
        .into_iter()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .collect();
    if keys.is_empty() {
        return;
    }
    let running = running_boxes(paths);
    if running.is_empty() {
        return;
    }
    let paths = paths.clone();
    std::thread::spawn(move || {
        for (provider, target, name) in running {
            let outcome = Ops::for_provider(&paths, &provider)
                .map_err(|error| error.message)
                .and_then(|ops| forget_on_box(&ops, &target, &keys));
            match outcome {
                Ok(()) => eprintln!(
                    "[gxserver] Cloud Boxes: took an unpaired device off {name} ({})",
                    target.box_id
                ),
                Err(error) => eprintln!(
                    "[gxserver] Cloud Boxes: couldn't take an unpaired device off {name} ({}) now; it goes at the box's next start ({error})",
                    target.box_id
                ),
            }
        }
    });
}

/// The devices `ticket` names that this computer no longer lists; none when the paired devices or
/// this computer's own box keys can't be read, so a read that failed never takes anyone off.
fn unpaired_since(paths: &GxserverPaths, ticket: &BoxBootTicket) -> Vec<String> {
    if !ticket.tailcat {
        return Vec::new();
    }
    let Some(paired) = boot_steps::paired_device_keys(paths) else {
        return Vec::new();
    };
    let home = super::home_keys::public_keys(paths);
    if home.is_empty() {
        return Vec::new();
    }
    ticket
        .allow_device_keys
        .iter()
        .filter(|key| !paired.contains(key) && !home.contains(key))
        .cloned()
        .collect()
}

/// After a box booted on `ticket`: takes off the devices unpaired while it was starting.
pub(super) fn catch_up_after_boot(
    paths: &GxserverPaths,
    ops: &Ops,
    target: &Target,
    ticket: &BoxBootTicket,
) {
    let stale = unpaired_since(paths, ticket);
    if stale.is_empty() {
        return;
    }
    if let Err(error) = forget_on_box(ops, target, &stale) {
        eprintln!(
            "[gxserver] Cloud Boxes: couldn't take a device unpaired during the start off {} ({error})",
            target.box_id
        );
    }
}
