//! Keeps every connected computer's Spaces and project groups alike, in the one list and in
//! Separate by machine
//! (the rule is gx-core `project_docs/shared.rs`): runs a pass when a document, a project or a
//! machine moved, writes what each computer lacks, and waits for each write to come back before the
//! next.
//!
//! CDXC:Spaces 2026-11-05 DECISION:
//! User (Q1354): "merge today's Spaces by name ... show a toast saying they should name their Spaces the same on each machine so they get grouped". The first pass of an app run that finds Spaces named differently on two computers says so once, after it has merged them.
//!
//! CDXC:Spaces 2026-11-05 WHY:
//! A computer's groups are written before its Spaces: gxserver drops a Space's group it does not hold yet, so a Spaces document naming a group that arrives a moment later would lose it. The Spaces document the pass planned is sent as planned once the groups come back, not planned again, because a pass over the half-written computer no longer sees a deleted group's Space. Before the first write of an app run every computer's two documents are saved to `<state>/backups/`, and nothing is written when that save fails.
//!
//! CDXC:Spaces 2026-11-05 WHY:
//! What the last pass left each computer with is kept in `<state>/backups/shared-spaces-last-<computers>.json`, one file per set of computers, so a removal made on another computer while this app was closed is read as that computer's edit after a restart rather than merged back in.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use std::collections::BTreeSet;
use std::path::PathBuf;

use ghostex_gx_core::{
    CollectionsDocument, MachineId, SharedDocsMachine, SharedDocsSnapshot, SpacesDocument,
    plan_shared_docs, shared_docs_state_from_json, shared_docs_state_to_json, shared_repo_keys,
};
use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;

/// How long a write may take to come back before the pass stops waiting for it.
const ECHO_WAIT: Duration = Duration::from_secs(30);
/// Writes to one computer that did not come back as sent, in a row, before the pass stops writing
/// to it until its documents move.
const MAX_UNECHOED_WRITES: u32 = 3;

/// What the passes of this app run did; `last` and `group_aliases` are also kept on disk.
#[derive(Default)]
pub(crate) struct SharedProjectDocsHost {
    /// The set of computers `last` was read from disk for.
    loaded_for: Option<String>,
    /// Groups the user deleted (Ungroup) since the last pass: removed on every computer.
    deleted_groups: BTreeSet<String>,
    /// The Spaces document planned for a computer whose groups are on their way.
    queued_spaces: BTreeMap<MachineId, SpacesDocument>,
    /// What each computer held after the last pass, to tell which one changed since.
    last: BTreeMap<MachineId, SharedDocsSnapshot>,
    /// A write still on its way: what the computer should echo, and since when.
    pending: BTreeMap<MachineId, (SharedDocsSnapshot, Instant)>,
    /// Writes in a row a computer did not echo as sent.
    unechoed: BTreeMap<MachineId, u32>,
    group_aliases: BTreeMap<(MachineId, String), String>,
    backed_up: bool,
    toast_shown: bool,
}

impl GhostexGpuiApp {
    /// One pass, when more than one computer is connected.
    pub(super) fn gx_store_sync_shared_project_docs(&mut self, cx: &mut gpui::Context<Self>) {
        // Separate by machine keeps the Spaces shared too, each tab showing its computer's
        // projects of them (Q1356), so the pass runs in both modes.
        let Some(machines) = self.gx_store_shared_docs_machines(cx) else {
            return;
        };
        if machines.len() < 2 {
            return;
        }
        let now = Instant::now();
        let pair = machines_key(&machines);
        let host = &mut self.gx_store.shared_project_docs;
        if host.loaded_for.as_deref() != Some(pair.as_str()) {
            let (last, aliases) = read_state(&pair);
            host.last = last;
            host.group_aliases = aliases;
            host.pending.clear();
            host.queued_spaces.clear();
            host.loaded_for = Some(pair.clone());
        }
        for machine in &machines {
            let Some((expected, since)) = host.pending.get(&machine.machine) else {
                continue;
            };
            let current = SharedDocsSnapshot::of(machine);
            if current == *expected {
                host.pending.remove(&machine.machine);
                host.unechoed.remove(&machine.machine);
            } else if now.duration_since(*since) < ECHO_WAIT {
                return;
            } else {
                // The computer kept something else: take its documents as they are, without
                // reading that as an edit made there.
                host.pending.remove(&machine.machine);
                host.queued_spaces.remove(&machine.machine);
                *host.unechoed.entry(machine.machine.clone()).or_default() += 1;
                host.last.insert(machine.machine.clone(), current);
            }
        }
        // A computer's groups came back: now its Spaces, exactly as planned.
        let queued = std::mem::take(&mut host.queued_spaces);
        if !queued.is_empty() {
            for (machine_id, spaces) in queued {
                let Some(machine) = machines
                    .iter()
                    .find(|machine| machine.machine == machine_id)
                else {
                    continue;
                };
                let mut expected = SharedDocsSnapshot::of(machine);
                expected.spaces = Some(spaces.state.clone());
                self.gx_store
                    .shared_project_docs
                    .pending
                    .insert(machine_id.clone(), (expected, now));
                self.gx_store_write_shared_spaces(&machine_id, spaces, cx);
            }
            return;
        }
        let deleted = std::mem::take(&mut host.deleted_groups);
        let plan = plan_shared_docs(&machines, &host.last, &host.group_aliases, &deleted);
        host.group_aliases = plan.group_aliases.clone();
        let mut snapshots = plan.snapshots.clone();
        let mut writes = Vec::new();
        for write in &plan.writes {
            if host.unechoed.get(&write.machine).copied().unwrap_or(0) < MAX_UNECHOED_WRITES {
                writes.push(write.clone());
            } else if let Some(machine) = machines.iter().find(|m| m.machine == write.machine) {
                // Not written: the computer keeps what it has, and that is not an edit made there.
                snapshots.insert(write.machine.clone(), SharedDocsSnapshot::of(machine));
            }
        }
        if !writes.is_empty() && !host.backed_up {
            if let Err(error) = back_up(&machines) {
                eprintln!("shared Spaces: not writing, the backup failed: {error}");
                // Asked again on the next pass, so the deletion is not lost with this one.
                self.gx_store.shared_project_docs.deleted_groups = deleted;
                return;
            }
            self.gx_store.shared_project_docs.backed_up = true;
        }
        for write in writes {
            let Some(machine) = machines
                .iter()
                .find(|machine| machine.machine == write.machine)
            else {
                continue;
            };
            let mut expected = SharedDocsSnapshot::of(machine);
            match (write.collections, write.spaces) {
                (Some(collections), spaces) => {
                    expected.collections = collections.state.clone();
                    if let Some(spaces) = spaces {
                        self.gx_store
                            .shared_project_docs
                            .queued_spaces
                            .insert(write.machine.clone(), spaces);
                    }
                    self.gx_store_write_shared_collections(&write.machine, collections, cx);
                }
                (None, Some(spaces)) => {
                    expected.spaces = Some(spaces.state.clone());
                    self.gx_store_write_shared_spaces(&write.machine, spaces, cx);
                }
                (None, None) => continue,
            }
            self.gx_store
                .shared_project_docs
                .pending
                .insert(write.machine.clone(), (expected, now));
        }
        let host = &mut self.gx_store.shared_project_docs;
        let wrote = !host.pending.is_empty();
        let moved = host.last != snapshots;
        host.last = snapshots;
        if moved {
            write_state(&pair, &host.last, &host.group_aliases);
        }
        if wrote && plan.names_differ && !host.toast_shown {
            host.toast_shown = true;
            self.dispatch_gpui_workspace_action_toast(
                "info",
                "Spaces from all your computers are one list now",
                "Spaces with the same name on two computers became one. Name a Space the same on each computer to keep them together.",
                cx,
            );
        }
    }

    /// The user deleted a project group (Ungroup) while the list is one: the next pass removes it
    /// from every computer, its projects keeping its Space.
    pub(crate) fn gx_store_note_shared_group_deleted(&mut self, collection_id: &str) {
        if self.gx_store.sidebar_list.last_inputs.host.merge_machines {
            self.gx_store
                .shared_project_docs
                .deleted_groups
                .insert(collection_id.to_string());
        }
    }

    /// Every computer the pass reads: this one, then each live machine of the one list whose
    /// documents it holds. `None` while this computer's groups have not been read yet.
    fn gx_store_shared_docs_machines(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Vec<SharedDocsMachine>> {
        if !self.gx_document_restored::<CollectionsDocument>(cx) {
            return None;
        }
        let guests: Vec<String> = self
            .gx_store
            .sidebar_list
            .last_inputs
            .host
            .machines
            .iter()
            .filter(|tab| !tab.is_local() && tab.fed && !tab.this_computer)
            .map(|tab| tab.machine_id.clone())
            .filter(|machine_id| {
                self.gx_store
                    .core
                    .presentation()
                    .machine(&MachineId::Remote(machine_id.clone()))
                    .is_some_and(|entry| {
                        entry.loaded().is_some()
                            && entry.connection().phase == ghostex_gx_core::ConnectionPhase::Live
                            && entry.side_state().project_collections.is_some()
                    })
            })
            .collect();
        let ids: Vec<MachineId> = std::iter::once(MachineId::Local)
            .chain(guests.iter().cloned().map(MachineId::Remote))
            .collect();
        let mut repos = shared_repo_keys(self.gx_store.core.presentation(), &ids);
        let store = &self.gx_store;
        let mut machines = vec![SharedDocsMachine {
            machine: MachineId::Local,
            spaces: store
                .spaces
                .sync
                .has_document()
                .then(|| store.spaces.sync.document().clone()),
            collections: store.collections.sync.document().clone(),
            repos: repos.remove(&MachineId::Local).unwrap_or_default(),
        }];
        for machine_id in guests {
            let machine = MachineId::Remote(machine_id.clone());
            machines.push(SharedDocsMachine {
                spaces: self.gx_store_remote_spaces(&machine_id),
                collections: self.gx_store_remote_collections(&machine_id),
                repos: repos.remove(&machine).unwrap_or_default(),
                machine,
            });
        }
        Some(machines)
    }

    fn gx_store_write_shared_collections(
        &mut self,
        machine: &MachineId,
        document: CollectionsDocument,
        cx: &mut gpui::Context<Self>,
    ) {
        match machine.remote_id() {
            Some(machine_id) => {
                let machine_id = machine_id.to_string();
                self.gx_store_send_remote_collections(&machine_id, &document, cx);
            }
            None => self.gx_document_edit::<CollectionsDocument>(document, cx),
        }
    }

    fn gx_store_write_shared_spaces(
        &mut self,
        machine: &MachineId,
        document: SpacesDocument,
        cx: &mut gpui::Context<Self>,
    ) {
        match machine.remote_id() {
            Some(machine_id) => {
                let machine_id = machine_id.to_string();
                self.gx_store_send_remote_spaces(&machine_id, &document, cx);
            }
            None => self.gx_document_edit::<SpacesDocument>(document, cx),
        }
    }
}

/// Every computer's Spaces and groups as they are before the first write, in one file under
/// `<state>/backups/`.
fn back_up(machines: &[SharedDocsMachine]) -> std::io::Result<()> {
    let mut documents = Map::new();
    for machine in machines {
        let id = machine
            .machine
            .remote_id()
            .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
            .to_string();
        documents.insert(
            id,
            json!({
                "spaces": machine.spaces.as_ref().map(SpacesDocument::to_wire_json),
                "projectCollections": machine.collections.to_wire_json(),
            }),
        );
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("backups")
        .join(format!("shared-spaces-{stamp}.json"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(&Value::Object(documents))?;
    ghostex_paths::durable_file::write_durably(&path, &bytes)
}

/// The set of computers a state file is for: this one and every other, by id.
fn machines_key(machines: &[SharedDocsMachine]) -> String {
    let mut ids: Vec<String> = machines
        .iter()
        .map(|machine| {
            machine
                .machine
                .remote_id()
                .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        })
        .collect();
    ids.sort();
    ids.join("+")
}

fn state_path(pair: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("backups")
        .join(format!("shared-spaces-last-{pair}.json"))
}

#[allow(clippy::type_complexity)]
fn read_state(
    pair: &str,
) -> (
    BTreeMap<MachineId, SharedDocsSnapshot>,
    BTreeMap<(MachineId, String), String>,
) {
    std::fs::read(state_path(pair))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|value| shared_docs_state_from_json(&value))
        .unwrap_or_default()
}

fn write_state(
    pair: &str,
    last: &BTreeMap<MachineId, SharedDocsSnapshot>,
    aliases: &BTreeMap<(MachineId, String), String>,
) {
    let path = state_path(pair);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            let bytes = serde_json::to_vec_pretty(&shared_docs_state_to_json(last, aliases))?;
            ghostex_paths::durable_file::write_durably(&path, &bytes)
        });
    if let Err(error) = written {
        eprintln!("shared Spaces: could not save what the last pass left: {error}");
    }
}
