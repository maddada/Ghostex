//! The machine tabs: which projects a machine would draw, and the counts its badge shows.
//!
//! The selected machine's list is built in full, so its counts come out of the built groups
//! (`assemble`). Every OTHER machine still has a tab with a badge, and building its whole list to
//! read two numbers off it would be the expensive way to ask a cheap question: a badge counts
//! sessions, and a session's activity is on the daemon row before any row is derived. So the counts
//! here walk the store directly, over exactly the sessions the list would draw.
//!
//! SEE-ALSO: the deleted sidebar page's `model.ts`, which counted
//! `state.groupOrder.filter(machine).flatMap(sessionIdsByGroup)` through `getGroupSessionSummary`.
//! Browser tabs are in that list too and always count zero (an idle row with no pending question),
//! so they are left out here rather than mirrored.

use std::collections::BTreeSet;

use crate::keys::MachineId;
use crate::presentation_store::{MachinePresentation, PresentationStore};

use super::inputs::SidebarHostInputs;
use super::membership::project_members;
use super::view::MachineSummary;

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
) -> MachineSummary {
    let mut summary = MachineSummary::default();
    let Some(machine) = store.machine(machine_id) else {
        return summary;
    };
    for project_id in drawn_project_ids(machine, parked) {
        // A chat project's user-made groups are never drawn, so their members are claimed out of
        // the project's own list and counted nowhere, exactly as `project_members` reports it.
        let emit_subgroups = !machine.is_chat_project(project_id);
        let members = project_members(store, machine, machine_id, project_id, emit_subgroups);
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
            if session.activity.as_str() == "working" {
                summary.working_count += 1;
            }
            if session.activity.as_str() == "attention" || session.pending_question_count > 0 {
                summary.attention_count += 1;
            }
            if session.background_work_detected_at.is_some()
                && session.activity.as_str() != "working"
                && session.activity.as_str() != "attention"
            {
                summary.background_work_count += 1;
            }
        }
    }
    summary
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
        drawn_project_ids(machine, host.parked_project_ids(machine_id))
            .into_iter()
            .any(|project_id| !machine.is_chat_project(project_id))
    })
}
