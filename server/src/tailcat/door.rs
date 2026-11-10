//! Who each Easy Connect helper lets in, and the door a new device comes through.
//!
//! CDXC:RemotePairing 2026-11-10 WHY:
//! The coordinator's brief after a user reported "a lot of reconnects": "showing a pairing code must not restart the host's Easy Connect helpers at all ... open the door for a new device only when it actually knocks, or add the new device's key without restarting the helper that carries everyone else", and phones on 11.2.1 and later must still pair. A helper takes its allow-list on its command line, so changing who it admits means starting it again, and a helper that starts again cuts every device on it (their clients never come back by themselves, `ghostex_paths::tailcat_forward`). Until now a pairing code on screen lifted the list of both helpers and took it back a minute later: two restarts each, 12 of the 33 helper restarts on one computer in a day, every one cutting every phone and computer connected. The allow-list stays in force exactly as the 2026-10-25 and 2026-11-08 decisions in supervisor.rs say; only how a new device gets past it changed, and the two helpers every paired device dials are no longer opened to unlisted keys at all.
//!
//! What happens instead:
//! - **The door.** A third helper with a key and address of its own, which exists only for a device that is not on the other two yet. While a pairing code is shown (or a device waits as Pending) it runs open, and the QR carries its address, so a new phone reaches `/api/pairDevice` through it; the other two helpers are not touched. Opening and closing it starts again only this helper.
//! - **An approved key is added without a restart where that would cut someone.** A helper that has carried a link keeps running with the list it has, and the key is withheld from it until it next starts for another reason (gxserver starting, a removed device, a changed port or relay), when it takes every saved key. A helper that has carried no link starts again with the key at once, which cuts nobody. A key that neither helper serves yet stays admitted at the door, and that device's address list names the door first (`easy_connect_addresses_for`), so the phone keeps working from the moment it is approved.
//! - **A knock.** The text code other computers connect with (and a QR made while the door could not run) names the first helper's address, where a new key is turned away. While a code is shown, the first key turned away at a helper is let in by starting that one helper again with it: one restart of one helper, only when a device really comes, instead of two of both for every code shown. Its client is still saying hello (tailcat repeats it every second for ten), so it gets in without trying again. A key that knocked and did not pair is dropped when the window closes.
//! - A removed device still restarts the helpers at once: that closes a door, and waiting would leave it open.
//! SEE-ALSO: supervisor.rs (the reconcile), link_log.rs (`refused_client_key`), server/src/remote_access/easy_connect_addresses.rs, server/src/remote_access/http.rs (`/api/remotePairingCode`), server/src/remote_access/paired_devices.rs (`add_paired_tailcat_client_key`).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::paths::GxserverPaths;

use super::binary::*;
use super::keys::*;
use super::supervisor::{TailcatLane, TailcatRuntime, TAILCAT_ALLOW_NONE};

/// The most new keys one helper lets in on a knock while one pairing window is open.
const KNOCKS_PER_WINDOW: usize = 4;
/// How long a helper that was started again still counts as carrying the devices its
/// predecessor carried: they take up to a minute to come back, and until then it looks idle.
const LINKS_RETURN_WITHIN: Duration = Duration::from_secs(5 * 60);

/// Which saved keys each of the two main helpers does not serve yet, and which unsaved keys it
/// let in on a knock.
#[derive(Default)]
pub(super) struct Admission {
    withheld_public: BTreeSet<String>,
    withheld_relay: BTreeSet<String>,
    knocked_public: BTreeSet<String>,
    knocked_relay: BTreeSet<String>,
}

impl Admission {
    fn withheld(&mut self, lane: TailcatLane) -> Option<&mut BTreeSet<String>> {
        match lane {
            TailcatLane::Public => Some(&mut self.withheld_public),
            TailcatLane::Relay => Some(&mut self.withheld_relay),
            TailcatLane::Door => None,
        }
    }

    fn knocked(&mut self, lane: TailcatLane) -> Option<&mut BTreeSet<String>> {
        match lane {
            TailcatLane::Public => Some(&mut self.knocked_public),
            TailcatLane::Relay => Some(&mut self.knocked_relay),
            TailcatLane::Door => None,
        }
    }

    /// The keys a main helper serves when it keeps running: the saved ones it is not withheld,
    /// and the ones that knocked.
    pub(super) fn kept_list(&mut self, lane: TailcatLane, saved: &[String]) -> Vec<String> {
        // A key that left the allow-list is owed to nobody any more.
        if let Some(withheld) = self.withheld(lane) {
            withheld.retain(|key| saved.contains(key));
        }
        let withheld = self.withheld(lane).cloned().unwrap_or_default();
        let mut keys: BTreeSet<String> = saved
            .iter()
            .filter(|key| !withheld.contains(*key))
            .cloned()
            .collect();
        keys.extend(self.knocked(lane).cloned().unwrap_or_default());
        keys.into_iter().collect()
    }

    /// The keys a main helper serves when it starts again: every saved one, and the ones that
    /// knocked. Nothing is withheld from it any more.
    pub(super) fn serve_all(&mut self, lane: TailcatLane, saved: &[String]) -> Vec<String> {
        if let Some(withheld) = self.withheld(lane) {
            withheld.clear();
        }
        let mut keys: BTreeSet<String> = saved.iter().cloned().collect();
        keys.extend(self.knocked(lane).cloned().unwrap_or_default());
        keys.into_iter().collect()
    }

    /// The saved keys no main helper serves: the door admits them. `relay_runs` is whether the
    /// second helper exists at all.
    pub(super) fn door_list(&self, saved: &[String], relay_runs: bool) -> Vec<String> {
        saved
            .iter()
            .filter(|key| {
                self.withheld_public.contains(*key)
                    && (!relay_runs || self.withheld_relay.contains(*key))
            })
            .cloned()
            .collect()
    }

    fn withhold(&mut self, lane: TailcatLane, key: &str) {
        if let Some(withheld) = self.withheld(lane) {
            withheld.insert(key.to_string());
        }
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
        if let Some(withheld) = self.withheld(lane) {
            withheld.clear();
        }
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
    if let (Some(host), Some(binary)) = (relay_host, resolve_client_tailcat_binary()) {
        let key_file = tailcat_door_key_file(paths, Some(host));
        if ensure_tailcat_relay_key_file(&binary, &key_file, host).is_ok() {
            return Some((binary, key_file));
        }
    }
    let binary = resolve_client_tailcat_binary().or_else(|| resolve_host_tailcat_binary(paths))?;
    let key_file = tailcat_door_key_file(paths, None);
    match ensure_tailcat_key_file(&binary, &key_file) {
        Ok(()) => Some((binary, key_file)),
        Err(error) => {
            eprintln!("[gxserver] could not create the Easy Connect door key: {error}");
            None
        }
    }
}

/// The door's address while it runs, and the relay its key is pinned to (`None`: the public ones).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorAddress {
    pub address: String,
    pub relay_host: Option<String>,
}

/// Which helpers let a key in right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LanesAdmitting {
    pub public: bool,
    pub relay: bool,
    pub door: bool,
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
    /// approval on a computer nobody is connected to starts both again with the new key, and the
    /// answer the new device reads a moment later must still name them.
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

    /// Which running helpers admit `key`.
    pub fn lanes_admitting(&self, key: &str) -> LanesAdmitting {
        let Ok(mut inner) = self.inner.lock() else {
            return LanesAdmitting::default();
        };
        let mut admits = |lane: TailcatLane| {
            let state = inner.lane(lane);
            state.running
                && state.spec.as_ref().is_some_and(|spec| {
                    spec.allowed_client_keys.is_empty()
                        || spec.allowed_client_keys.iter().any(|listed| listed == key)
                })
        };
        LanesAdmitting {
            public: admits(TailcatLane::Public),
            relay: admits(TailcatLane::Relay),
            door: admits(TailcatLane::Door),
        }
    }

    /// Called before a newly approved device's `key` is saved on the allow-list: a main helper
    /// that has carried a link and does not serve the key yet keeps running without it.
    pub fn withhold_new_key_from_busy_helpers(&self, key: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        for lane in [TailcatLane::Public, TailcatLane::Relay] {
            let state = inner.lane(lane);
            let Some(spec) = state.spec.as_ref() else {
                continue;
            };
            let serves = spec.allowed_client_keys.iter().any(|listed| listed == key);
            let busy = state.peers.as_ref().is_some_and(|peers| !peers.is_empty())
                || state
                    .links_cut_at
                    .is_some_and(|at| at.elapsed() < LINKS_RETURN_WITHIN);
            if !serves && busy {
                inner.admission.withhold(lane, key);
            }
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
