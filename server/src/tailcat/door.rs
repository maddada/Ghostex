//! Who each Easy Connect helper lets in, and the door a new device comes through.
//!
//! CDXC:RemotePairing 2026-11-10 WHY:
//! The coordinator's brief after a user reported "a lot of reconnects": "showing a pairing code must not restart the host's Easy Connect helpers at all ... open the door for a new device only when it actually knocks, or add the new device's key without restarting the helper that carries everyone else", and phones on 11.2.1 and later must still pair. A helper takes its allow-list on its command line, so changing who it admits means starting it again, and a helper that starts again cuts every device on it (their clients never come back by themselves, `ghostex_paths::tailcat_forward`). Until now a pairing code on screen lifted the list of both helpers and took it back a minute later: two restarts each, 12 of the 33 helper restarts on one computer in a day, every one cutting every phone and computer connected. The allow-list stays in force exactly as the 2026-10-25 and 2026-11-08 decisions in supervisor.rs say; only how a new device gets past it changed, and the two helpers every paired device dials are no longer opened to unlisted keys at all.
//!
//! What happens instead:
//! - **The door.** A third helper with a key and address of its own, which exists only for a device that is not on the other two yet. While a pairing code is shown (or a device waits as Pending) it runs open, and the QR carries its address, so a new phone reaches `/api/pairDevice` through it; the other two helpers are not touched. Opening and closing it starts again only this helper.
//! - **An approved key starts the two helpers again once.** The new device's key goes on the allow-list when its request is approved, and both helpers take it by starting again: the devices connected reconnect once, at the moment the owner approves, where a code shown used to cost them two restarts whether anybody paired or not. The address list the new device is given names those two helpers to dial (since 2026-11-11 the addresses of both doors a code can carry follow them, last and flagged, only so a phone can recognise this computer at its next scan; see `remember_door_address`), so nothing it dials first stops answering when the door closes. The first version of this (4ac828ff7, the same day) held an approved key back from a helper that carried links and let the device live on the door instead; the review of it and the rig showed why that does not hold: tailcat never says when a link leaves, so a helper that once carried one never looks free again and the device stayed on the door alone, and a phone whose saved list named the door first waited 20 s on it (its hello timeout) the first time the door had stopped.
//! - **A knock.** The text code other computers connect with (and a QR made while the door could not run) names the first helper's address, where a new key is turned away. While a code is shown, the first key turned away at a helper is let in by starting that one helper again with it: one restart of one helper, only when a device really comes, instead of two of both for every code shown. Its client is still saying hello (tailcat repeats it every second for ten), so it gets in without trying again. A key that knocked and did not pair is dropped when the window closes.
//! - A removed device still restarts the helpers at once: that closes a door, and waiting would leave it open.
//! SEE-ALSO: supervisor.rs (the reconcile), link_log.rs (`refused_client_key`), server/src/remote_access/easy_connect_addresses.rs, server/src/remote_access/http.rs (`/api/remotePairingCode`), server/src/remote_access/pair_device.rs (the approval adds the key).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::paths::GxserverPaths;

use super::binary::*;
use super::keys::*;
use super::supervisor::{TailcatLane, TailcatRuntime, TAILCAT_ALLOW_NONE};

/// The most new keys one helper lets in on a knock while one pairing window is open.
const KNOCKS_PER_WINDOW: usize = 4;
/// The unsaved keys each of the two main helpers let in on a knock.
#[derive(Default)]
pub(super) struct Admission {
    knocked_public: BTreeSet<String>,
    knocked_relay: BTreeSet<String>,
}

impl Admission {
    fn knocked(&mut self, lane: TailcatLane) -> Option<&mut BTreeSet<String>> {
        match lane {
            TailcatLane::Public => Some(&mut self.knocked_public),
            TailcatLane::Relay => Some(&mut self.knocked_relay),
            TailcatLane::Door => None,
        }
    }

    /// The keys a main helper serves: every saved one, and the ones that knocked at it.
    pub(super) fn served_list(&mut self, lane: TailcatLane, saved: &[String]) -> Vec<String> {
        let mut keys: BTreeSet<String> = saved.iter().cloned().collect();
        keys.extend(self.knocked(lane).cloned().unwrap_or_default());
        keys.into_iter().collect()
    }

    /// Lets `key` in at `lane` on its knock; false when it is in already or the window's knocks
    /// are used up.
    fn knock(&mut self, lane: TailcatLane, key: &str) -> bool {
        let Some(knocked) = self.knocked(lane) else {
            return false;
        };
        if knocked.len() >= KNOCKS_PER_WINDOW {
            return false;
        }
        knocked.insert(key.to_string())
    }

    /// The pairing window closed: a key that knocked and was approved is a saved key now, and
    /// one that was not goes.
    pub(super) fn forget_knocks(&mut self) {
        self.knocked_public.clear();
        self.knocked_relay.clear();
    }

    /// A main helper stopped for good (its relay was turned off): nothing is owed to it.
    pub(super) fn forget_lane(&mut self, lane: TailcatLane) {
        if let Some(knocked) = self.knocked(lane) {
            knocked.clear();
        }
    }
}

/// The saved allow-list without the `none` placeholder an empty list is stored as.
pub(super) fn saved_client_keys(saved: &[String]) -> Vec<String> {
    saved
        .iter()
        .map(|key| key.trim())
        .filter(|key| !key.is_empty() && *key != TAILCAT_ALLOW_NONE)
        .map(str::to_string)
        .collect()
}

/// Where the door's helper writes its address.
pub fn tailcat_door_address_file(paths: &GxserverPaths) -> PathBuf {
    paths.tailcat_dir.join("door-address.txt")
}

/// The door's key on the public relays, or pinned to the relay `host`. Made once each, like the
/// other helpers' keys, so a device that was given the door's address finds it again.
pub fn tailcat_door_key_file(paths: &GxserverPaths, host: Option<&str>) -> PathBuf {
    match host {
        Some(host) => paths
            .tailcat_dir
            .join("door")
            .join(format!("{host}.private.json")),
        None => paths.tailcat_dir.join("door").join("public.private.json"),
    }
}

/// The helper and key a door that is not running yet starts with: on the second helper's relay
/// when that relay answered its last check (a new phone then never touches the public relays, as
/// with the QR until now), else on the public relays.
pub(super) fn door_helper(
    paths: &GxserverPaths,
    relay_host: Option<&str>,
) -> Option<(PathBuf, PathBuf)> {
    relay_host
        .and_then(|host| door_key(paths, Some(host)))
        .or_else(|| door_key(paths, None))
}

/// The door's helper and key pinned to the relay `host` (only with a tailcat that can use one), or
/// on the public relays; makes the key when it is missing. `None` when it cannot be made.
fn door_key(paths: &GxserverPaths, host: Option<&str>) -> Option<(PathBuf, PathBuf)> {
    let key_file = tailcat_door_key_file(paths, host);
    let Some(host) = host else {
        let binary =
            resolve_client_tailcat_binary().or_else(|| resolve_host_tailcat_binary(paths))?;
        return match make_door_key(&binary, &key_file, None) {
            Ok(()) => Some((binary, key_file)),
            Err(error) => {
                eprintln!("[gxserver] could not create the Easy Connect door key: {error}");
                None
            }
        };
    };
    let binary = resolve_client_tailcat_binary()?;
    make_door_key(&binary, &key_file, Some(host))
        .ok()
        .map(|()| (binary, key_file))
}

/// Makes the door's key when it is missing, and keeps the address tailcat prints for it.
fn make_door_key(
    binary: &std::path::Path,
    key_file: &std::path::Path,
    host: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(address) = make_tailcat_key_file(binary, key_file, host)? {
        save_door_address(key_file, &address);
    }
    Ok(())
}

/*
CDXC:RemotePairing 2026-11-11 WHY:
A phone recognises a computer by the server key inside an Easy Connect address: the address it paired with, or any address in the list the computer gave it (apps/mobile/app/src/machines/computerIdentity.ts). Since 11.3.0 the QR carries the door's address, whose key is on no phone's list, so a phone that scanned a new code through "Add a computer" for a computer it already had could not tell and added a second entry. The computer replaces that phone's earlier pairing, so the old entry was dead, and both entries dialled with the phone's one key, two tunnels that stall each other: the user's own iPhone showed exactly this (one entry Connected, one "SSH needs a key or password"), and a user with constant reconnects very likely had it. So the door's address is remembered (its key never changes, so neither does the key in its address) and named last in every address list (`easy_connect_addresses_with_doors`; both doors since finding 3 of its review, see `remember_door_address`): a phone on the old app, which cannot be updated over the air, then finds its entry by it at the next scan and pairs it again instead of adding one. The address is remembered when the key is made (tailcat prints it), each time the door publishes it, and from the file 11.3.0 left behind; the key is made when Easy Connect starts, so the address is known before the first code is shown. Nothing about the door itself changes: it still runs only while a code is shown and lets nobody in.
SEE-ALSO: server/src/remote_access/easy_connect_addresses.rs, supervisor.rs (`apply`, `set_token`).
*/
/// Where the address of the door key `key_file` is kept (`<name>.address.txt` beside it).
fn door_saved_address_file(key_file: &std::path::Path) -> PathBuf {
    let name = key_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("door.private.json");
    key_file.with_file_name(format!(
        "{}.address.txt",
        name.strip_suffix(".private.json").unwrap_or(name)
    ))
}

/// Remembers `address` as the address of the door key `key_file`.
pub(super) fn save_door_address(key_file: &std::path::Path, address: &str) {
    let address = address.trim();
    if address.starts_with("tc") {
        let _ = std::fs::write(door_saved_address_file(key_file), address);
    }
}

fn saved_door_address(key_file: &std::path::Path) -> Option<String> {
    let address = std::fs::read_to_string(door_saved_address_file(key_file)).ok()?;
    let address = address.trim();
    address.starts_with("tc").then(|| address.to_string())
}

/// Whether `address` is an address of the key in `key_file`: its bytes hold the key's public half.
fn address_names_key(address: &str, key_file: &std::path::Path) -> bool {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let public = std::fs::read_to_string(key_file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| {
            json.pointer("/Public/ServerPublic")
                .and_then(serde_json::Value::as_str)
                .and_then(|key| key.strip_prefix("nodekey:"))
                .map(str::to_string)
        });
    let Some(public) = public else {
        return false;
    };
    let key: Option<Vec<u8>> = (0..public.len() / 2)
        .map(|index| u8::from_str_radix(public.get(index * 2..index * 2 + 2)?, 16).ok())
        .collect();
    let (Some(key), Some(encoded)) = (key, address.trim().strip_prefix("tc")) else {
        return false;
    };
    let Ok(bytes) = URL_SAFE_NO_PAD.decode(encoded.trim_end_matches('=')) else {
        return false;
    };
    key.len() == 32
        && bytes
            .windows(key.len())
            .any(|window| window == key.as_slice())
}

/*
CDXC:RemotePairing 2026-11-11 WHY:
Review of the first version (finding 3): a code carries one of two doors, the one pinned to the relay when that relay answered its last check and the one on the public relays otherwise (`door_helper`, supervisor.rs `apply_door`), but only one door's address was remembered (the first one found) and only one was listed. After the user switched relay, or while the relay was down so the code fell back to the public door, the listed door could differ from the one in the code, and an old phone added a second entry on that scan. Both doors are remembered and both are listed, each flagged as a door, so whichever door a code names is on the list a phone already has. Doors pinned to a relay the computer no longer uses are not listed: no code can carry them.
*/
/// The doors a code can carry: the one pinned to `relay_host` (when there is one and a tailcat
/// that can use it) and the one on the public relays. Makes each key that is missing and makes
/// sure each address is remembered. Called when Easy Connect starts; cheap once known.
pub(super) fn remember_door_address(paths: &GxserverPaths, relay_host: Option<&str>) {
    // Made by 11.3.0, which kept only the address of the door that ran last.
    let last_published = std::fs::read_to_string(tailcat_door_address_file(paths)).ok();
    for host in relay_host.into_iter().map(Some).chain([None]) {
        let key_file = tailcat_door_key_file(paths, host);
        if saved_door_address(&key_file).is_some() {
            continue;
        }
        if key_file.is_file() {
            // tailcat prints an address only when it makes a key; one 11.3.0 made earlier is
            // learned here, or the next time that door runs (`save_door_address`, supervisor.rs).
            if let Some(address) = last_published
                .as_deref()
                .filter(|address| address_names_key(address, &key_file))
            {
                save_door_address(&key_file, address);
            }
            continue;
        }
        let _ = door_key(paths, host);
    }
}

/// The addresses of every door a code can carry, when they are known: the one pinned to
/// `relay_host` first (a code carries it whenever that relay answers), then the one on the public
/// relays.
pub fn known_door_addresses(paths: &GxserverPaths, relay_host: Option<&str>) -> Vec<DoorAddress> {
    let pinned = relay_host.and_then(|host| {
        saved_door_address(&tailcat_door_key_file(paths, Some(host))).map(|address| DoorAddress {
            address,
            relay_host: Some(host.to_string()),
        })
    });
    let public =
        saved_door_address(&tailcat_door_key_file(paths, None)).map(|address| DoorAddress {
            address,
            relay_host: None,
        });
    pinned.into_iter().chain(public).collect()
}

/// The door's address while it runs, and the relay its key is pinned to (`None`: the public ones).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorAddress {
    pub address: String,
    pub relay_host: Option<String>,
}

impl TailcatRuntime {
    /// The door's address, when its helper runs and has published it.
    pub fn door_address(&self) -> Option<DoorAddress> {
        let mut inner = self.inner.lock().ok()?;
        let relay_host = inner.door_relay_host.clone();
        let door = inner.lane(TailcatLane::Door);
        if !door.running {
            return None;
        }
        Some(DoorAddress {
            address: door.token.clone()?,
            relay_host,
        })
    }

    /// The door's address once it runs open (a pairing window is held open), waiting up to
    /// `timeout` for a helper that is just starting. `None` when the door cannot run.
    pub fn wait_for_open_door(&self, timeout: Duration) -> Option<DoorAddress> {
        let deadline = Instant::now() + timeout;
        loop {
            let open = self.inner.lock().ok().is_some_and(|mut inner| {
                inner
                    .lane(TailcatLane::Door)
                    .spec
                    .as_ref()
                    .is_some_and(|spec| spec.allowed_client_keys.is_empty())
            });
            if !open {
                return None;
            }
            if let Some(address) = self.door_address() {
                return Some(address);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits up to `timeout` for a main helper that just started to publish its address. An
    /// approval starts both again with the new key, and the answer the new device reads a moment
    /// later must name them: they are the only addresses it is given.
    pub fn wait_for_main_addresses(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let starting = self.inner.lock().ok().is_some_and(|mut inner| {
                [TailcatLane::Public, TailcatLane::Relay]
                    .into_iter()
                    .any(|lane| {
                        let state = inner.lane(lane);
                        state.spec.is_some() && state.token.is_none()
                    })
            });
            if !starting || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// A helper turned `key` away. While a pairing code is shown that is a new device knocking:
    /// the helper it knocked at starts again with it.
    pub(super) fn note_refused(&self, lane: TailcatLane, generation: u64, key: &str) {
        let paths = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let open = inner
                .pairing_open_until
                .is_some_and(|until| until > Instant::now());
            if !open || inner.lane(lane).generation != generation {
                return;
            }
            if !inner.admission.knock(lane, key) {
                return;
            }
            inner.paths.clone()
        };
        let Some(paths) = paths else {
            return;
        };
        let runtime = self.clone();
        // Off the helper's log reader, which ends with the helper this starts again.
        std::thread::spawn(move || {
            super::status::start_tailcat_from_persisted_state(&paths, &runtime)
        });
    }
}
