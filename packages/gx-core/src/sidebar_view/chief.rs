//! Chief's rows: the session each computer flags `isChief` (server/src/chief/), drawn under the
//! Spaces bar instead of in its project's list (`PresentationSession::listed_in_sidebar`).
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1734, Q1741, Q1747): Chief is "its own row but under spaces bar (stays static while switching space)", one per computer. With computers combined the row shows this computer's Chief with a "+N" that unfolds the other computers' Chief rows under it, the way a coordinator's threads fold; with Separate by machine on, each computer's tab shows its own computer's Chief.
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1911, Q1993): Main Hub's row is the header of the home project (the project at the home folder, server/src/chief/home.rs), with the home's loose sessions under it like a project's. With computers combined it is "one list with a small computer chip on each chat instead of one header per computer (keep the Main Hub row as the header)": every computer's home is merged into this computer's entry (merged.rs, `identity_of`), each other computer's chat carrying its chip and its own Main Hub chat listed there too. Separate by machine keeps one computer's home per tab. A computer with a Chief but no home project (an older gxserver) still gets the plain row under the Spaces bar. Supersedes one header per computer (Q1913).
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/chief.rs (the renderer), apps/mobile/app (the phone's row).

use crate::keys::{MachineId, SessionKey};
use crate::presentation_store::PresentationStore;
use crate::LOCAL_MACHINE_ID;

use super::groups::FocusKey;
use super::inputs::SidebarInputs;
use super::rows::{session_row, RowContext};
use super::view::GroupView;
use super::view::SessionRow;

/// One computer's Chief.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChiefRowView {
    /// The machine tab id (`local` for this computer).
    pub machine_id: String,
    /// The computer's name, for the folded rows ("Chief · Hetzner").
    pub machine_label: String,
    pub is_local: bool,
    /// Its chat is the one open in the work area.
    pub is_focused: bool,
    pub row: SessionRow,
}

/// The Chief rows to draw: the selected machine's, or with `merging` every fed machine's, this
/// computer first. A machine whose Chief was never opened has none.
pub(crate) fn chief_rows(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    selected: &MachineId,
    merging: bool,
    focus: &FocusKey,
    context: &RowContext<'_>,
) -> Vec<ChiefRowView> {
    let mut machines: Vec<MachineId> = vec![selected.clone()];
    if merging {
        machines = std::iter::once(MachineId::Local)
            .chain(
                inputs
                    .host
                    .machines
                    .iter()
                    .filter(|tab| !tab.is_local() && tab.fed && !tab.this_computer)
                    .map(|tab| MachineId::Remote(tab.machine_id.clone())),
            )
            .collect();
    }
    machines
        .into_iter()
        .filter_map(|machine| chief_row_for(store, inputs, &machine, focus, context))
        .collect()
}

/// One home group's header: the computer it is on and that computer's Chief.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeHeaderView {
    /// The home project's group.
    pub group_id: String,
    /// The machine tab id (`local` for this computer).
    pub machine_id: String,
    /// The computer's name, which the header shows with the computers combined.
    pub machine_label: String,
    pub is_local: bool,
    /// That computer's Chief; `None` before its first open.
    pub chief: Option<ChiefRowView>,
}

/// The headers of the home groups the list draws, in list order.
pub(crate) fn home_headers(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    groups: &[GroupView],
    focus: &FocusKey,
    context: &RowContext<'_>,
) -> Vec<HomeHeaderView> {
    groups
        .iter()
        .filter(|group| {
            group
                .core
                .project_context
                .as_ref()
                .is_some_and(|project| project.is_home)
                && super::assemble::owner_project_group_id(&group.core.group_id).is_none()
        })
        .map(|group| {
            let machine = group
                .core
                .remote_machine
                .as_ref()
                .map(|remote| MachineId::Remote(remote.machine_id.clone()))
                .unwrap_or(MachineId::Local);
            let machine_id = machine.remote_id().unwrap_or(LOCAL_MACHINE_ID).to_string();
            HomeHeaderView {
                group_id: group.core.group_id.clone(),
                machine_label: machine_label(inputs, &machine_id),
                is_local: machine.is_local(),
                chief: chief_row_for(store, inputs, &machine, focus, context),
                machine_id,
            }
        })
        .collect()
}

/// The machines (of `chiefs`) that have a home project, drawn or not: their Chief is a home
/// header, never the row under the Spaces bar.
pub(crate) fn machines_with_home(
    store: &PresentationStore,
    chiefs: &[ChiefRowView],
) -> Vec<String> {
    chiefs
        .iter()
        .filter(|chief| {
            let machine = if chief.is_local {
                MachineId::Local
            } else {
                MachineId::Remote(chief.machine_id.clone())
            };
            store
                .machine(&machine)
                .and_then(|entry| entry.loaded())
                .is_some_and(|loaded| loaded.projects().iter().any(|project| project.is_home))
        })
        .map(|chief| chief.machine_id.clone())
        .collect()
}

fn machine_label(inputs: &SidebarInputs, machine_id: &str) -> String {
    inputs
        .host
        .machine(machine_id)
        .map(|tab| tab.label.trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "This computer".to_string())
}

/// `machine`'s Chief, when it has one.
fn chief_row_for(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    machine: &MachineId,
    focus: &FocusKey,
    context: &RowContext<'_>,
) -> Option<ChiefRowView> {
    let entry = store.machine(machine)?;
    let loaded = entry.loaded()?;
    let session = loaded.server_sessions().find(|session| session.is_chief)?;
    // The local patch (a rename, a read mark) applies to it like to any row; a session
    // hidden locally is not drawn.
    let session = entry.effective_session(&session.project_id, &session.session_id)?;
    let key = SessionKey {
        machine: machine.clone(),
        project_id: session.project_id.clone(),
        session_id: session.session_id.clone(),
    };
    let machine_id = machine.remote_id().unwrap_or(LOCAL_MACHINE_ID).to_string();
    let machine_label = machine_label(inputs, &machine_id);
    let is_focused = focus.focused_session_id.as_deref() == Some(session.session_id.as_str())
        && focus.active_project_id.as_deref() == Some(session.project_id.as_str());
    Some(ChiefRowView {
        is_local: machine.is_local(),
        is_focused,
        row: session_row(key, &session, None, None, context),
        machine_id,
        machine_label,
    })
}

/// Moves the Chief of Staff row into the home project's group when its session lives in another
/// project: a Chief started before the home existed runs in the Ghostex config folder's project
/// until its next New Chat, and its row still belongs at the top of Main Hub.
pub(crate) fn lift_chief_into_home(plans: &mut [super::groups::GroupPlan]) {
    let Some(home) = plans.iter().position(|plan| {
        plan.kind == super::groups::GroupKind::Project
            && plan.project.as_ref().is_some_and(|project| project.is_home)
    }) else {
        return;
    };
    for index in 0..plans.len() {
        if index == home {
            continue;
        }
        if let Some(at) = plans[index].rows.iter().position(|row| row.row.is_chief) {
            let row = plans[index].rows.remove(at);
            plans[home].rows.insert(0, row);
            return;
        }
    }
}
