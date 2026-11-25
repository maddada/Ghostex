//! The machine tabs: which projects a machine would draw, and the counts its badge shows.
//!
//! The selected machine's list is built in full, so its counts come out of the built groups
//! (`assemble`). Every OTHER machine still has a tab with a badge, and building its whole list to
//! read its counts would be the expensive way to ask a cheap question. The counts here walk the
//! store directly, over exactly the sessions the list would draw, and share the row build's
//! side-chat rollup so each visible parent carries its side chats' questions and approvals.
//!
//! SEE-ALSO: the deleted sidebar page's `model.ts`, which counted
//! `state.groupOrder.filter(machine).flatMap(sessionIdsByGroup)` through `getGroupSessionSummary`.
//! Browser tabs are in that list too and always count zero (an idle row with no pending question),
//! so they are left out here rather than mirrored.

use std::collections::BTreeSet;

use crate::keys::{MachineId, ProjectKey};
use crate::presentation_store::{MachinePresentation, PresentationStore};

use super::collections::CollectionsState;
use super::inputs::{SidebarHostInputs, SidebarUiState, LOCAL_MACHINE_ID};
use super::membership::project_members;
use super::rows::side_chat_needs;
use super::view::{tile_status, MachineSummary};

/// The projects a machine draws: everything loaded, minus the ones it parked as Recent Projects
/// and the ones a local close hid. The same filter `build_project_meta` applies before it orders
/// them, without the ordering, which a count does not need.
fn drawn_project_ids<'a>(
    machine: &'a MachinePresentation,
    parked: &BTreeSet<String>,
) -> Vec<&'a str> {
    let Some(loaded) = machine.loaded() else {
        return Vec::new();
    };
    loaded
        .projects()
        .iter()
        .map(|project| project.project_id.as_str())
        .filter(|project_id| {
            !parked.contains(*project_id)
                && !machine.is_project_hidden(project_id)
                && machine
                    .domain_project(project_id)
                    .and_then(|project| project.get("isRecentProject"))
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
        })
        .collect()
}

/// The working and attention counts of one machine's tab.
///
/// CDXC:Sidebar 2026-09-20 WHY:
/// This is the cheap way to ask a cheap question and it is still O(projects x groups): every
/// project calls `project_members`, which scans the machine's whole group list. At thirty-five
/// projects that is about 1.2k group scans per recount, and a recount runs whenever that machine's
/// rows move. It is only ever paid for a machine the list is NOT built for (the selected one's
/// counts come out of its built groups), and it is cached until that machine's rows move, which is
/// what keeps it off a one-machine sidebar entirely. Measure it before optimising it: the first
/// machine anyone enables will say whether a per-machine session index is worth its invalidation.
pub(crate) fn machine_tab_summary(
    store: &PresentationStore,
    machine_id: &MachineId,
    parked: &BTreeSet<String>,
    ui: &SidebarUiState,
    collections: &CollectionsState,
) -> MachineSummary {
    let mut summary = MachineSummary::default();
    let Some(machine) = store.machine(machine_id) else {
        return summary;
    };
    let side_chat_needs = side_chat_needs(store, machine_id);
    let drawn = drawn_project_ids(machine, parked);
    let section_key = match machine_id.remote_id() {
        None => LOCAL_MACHINE_ID.to_string(),
        Some(remote) => format!("remote:{remote}"),
    };
    let collection_id_by_project =
        hidden_collection_lookup(machine, parked, ui, collections, &section_key, &drawn);
    let home_project = machine.loaded().and_then(|loaded| {
        loaded
            .projects()
            .iter()
            .find(|project| project.is_home && !project.is_help_home)
            .map(|project| {
                (
                    project.project_id.as_str(),
                    ProjectKey {
                        machine: machine_id.clone(),
                        project_id: project.project_id.clone(),
                    }
                    .to_sidebar_group_id(),
                )
            })
    });
    for project_id in drawn {
        // Autopilot's supervisor project is never drawn on its own; its sessions live in Main Hub
        // while Main Hub is on and stay out of the counts while Main Hub is off.
        let is_autopilot = machine
            .loaded()
            .and_then(|loaded| loaded.project(project_id))
            .is_some_and(|project| project.autopilot_supervisor_project);
        let (counted_project_id, group_id) = if is_autopilot {
            let Some((home_id, home_group_id)) = home_project.as_ref() else {
                continue;
            };
            (*home_id, home_group_id.clone())
        } else {
            (
                project_id,
                ProjectKey {
                    machine: machine_id.clone(),
                    project_id: project_id.to_string(),
                }
                .to_sidebar_group_id(),
            )
        };
        // A project the user hid, or one filed in a collection the user hid, adds nothing to the
        // tab's dots (`assemble::counts_toward_dots`).
        if !super::assemble::counts_toward_dots(
            ui,
            &section_key,
            &group_id,
            collection_id_by_project
                .as_ref()
                .and_then(|lookup| lookup.get(counted_project_id))
                .map(String::as_str),
        ) {
            continue;
        }
        let members = project_members(store, machine, machine_id, project_id);
        let session_ids = members.session_ids.iter().chain(
            members
                .subgroups
                .iter()
                .flat_map(|subgroup| subgroup.session_ids.iter()),
        );
        for session_id in session_ids {
            let Some(session) = machine.effective_session(project_id, session_id) else {
                continue;
            };
            let side_chats = side_chat_needs
                .get(&(session.project_id.clone(), session.session_id.clone()))
                .copied()
                .unwrap_or_default();
            // `SessionRow::tile_status`, after the same side-chat rollup as the row.
            summary.count(tile_status(
                session.awaiting_approval || side_chats.approval,
                super::rows::session_failed(&session),
                ghostex_gx_protocol::session_has_question(
                    session.pending_question_count + side_chats.questions,
                    session.awaiting_answer || side_chats.awaiting_answer,
                ),
                session.activity.as_str(),
                session.background_work_detected_at.is_some(),
            ));
        }
    }
    summary
}

/// The collection each of a machine's drawn projects is filed in (a worktree by its parent's), as
/// the list files them (`CollectionsState::collection_id_by_project`). `None` while none of the
/// machine's collections is hidden, or Show hidden is on: then no collection keeps a project out
/// of the counts, and the worktree parents, which take the machine's project facts, are not
/// worked out.
fn hidden_collection_lookup(
    machine: &MachinePresentation,
    parked: &BTreeSet<String>,
    ui: &SidebarUiState,
    collections: &CollectionsState,
    section_key: &str,
    drawn: &[&str],
) -> Option<std::collections::BTreeMap<String, String>> {
    let any_hidden = !ui.show_hidden
        && collections.collections.iter().any(|collection| {
            ui.hidden_items
                .collection_keys
                .contains(&format!("{section_key}:{}", collection.collection_id))
        });
    if !any_hidden {
        return None;
    }
    let meta = super::projects::build_project_meta(machine, &[], parked);
    let project_ids: Vec<String> = drawn.iter().map(|id| id.to_string()).collect();
    Some(collections.collection_id_by_project(
        &project_ids,
        &|project_id| Some(project_id.to_string()),
        &|project_id| {
            meta.overlay(project_id)
                .and_then(|overlay| overlay.worktree.as_ref())
                .map(|worktree| worktree.parent_project_id.clone())
        },
    ))
}

/// Whether ANY machine the store holds draws a project group, which is what the empty state's
/// "have we ever seen a project" test asks.
///
/// `hasKnownSidebarProjectInventory` reads `workspaceGroupIds`, which spans every machine, so a
/// user whose only projects live on a remote machine must not be shown first-run copy.
pub(crate) fn any_machine_draws_a_project(
    store: &PresentationStore,
    host: &SidebarHostInputs,
) -> bool {
    store.machines().any(|(machine_id, machine)| {
        !drawn_project_ids(machine, host.parked_project_ids(machine_id)).is_empty()
    })
}
