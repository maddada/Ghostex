//! Chief's rows: the session each computer flags `isChief` (server/src/chief/), drawn under the
//! Spaces bar instead of in its project's list (`PresentationSession::listed_in_sidebar`).
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1734, Q1741, Q1747): Chief is "its own row but under spaces bar (stays static while switching space)", one per computer. With computers combined the row shows this computer's Chief with a "+N" that unfolds the other computers' Chief rows under it, the way a coordinator's threads fold; with Separate by machine on, each computer's tab shows its own computer's Chief.
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1911, Q1993): Main Hub's row is the header of the home project (the project at the home folder, server/src/chief/home.rs), with the home's loose sessions under it like a project's. With computers combined it is "one list with a small computer chip on each chat instead of one header per computer (keep the Main Hub row as the header)": every computer's home is merged into this computer's entry (merged.rs, `identity_of`), each other computer's chat carrying its chip and its own Main Hub chat listed there too. Separate by machine keeps one computer's home per tab. A computer with a Chief but no home project (an older gxserver) still gets the plain row under the Spaces bar. Supersedes one header per computer (Q1913).
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/chief.rs (the renderer), apps/mobile/app (the phone's row).

use std::collections::BTreeSet;

use crate::keys::{MachineId, SessionKey};
use crate::presentation_store::PresentationStore;
use crate::LOCAL_MACHINE_ID;

use super::groups::FocusKey;
use super::inputs::{SectionId, SidebarInputs};
use super::rows::{session_row, RowContext};
use super::view::{GroupView, MachineFold, SessionRow, SessionView};

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
        .unwrap_or_else(|| "Local".to_string())
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
/// until its next New Chat, and its row still belongs at the top of Main Hub. Ask Ghostex AI's
/// chats, which live in that same project (`is_help_home`), join the home group of their computer
/// too while Main Hub is on, so turning it on moves nothing (CDXC:Chief 2026-11-19 in
/// server/src/chief/home.rs); while it is off that project is the home itself. Autopilot's
/// supervisor sessions (`is_autopilot`) join their computer's real Main Hub (`is_home &&
/// !is_help_home`) under the Autopilot section; while Main Hub is off they stay hidden from the
/// sidebar rather than keeping a Main Hub header open.
pub(crate) fn lift_into_home(plans: &mut [super::groups::GroupPlan], focus: &mut FocusKey) {
    let is_home_plan = |plan: &super::groups::GroupPlan| {
        plan.kind == super::groups::GroupKind::Project
            && plan.project.as_ref().is_some_and(|project| project.is_home)
    };
    let is_real_home_plan = |plan: &super::groups::GroupPlan| {
        plan.kind == super::groups::GroupKind::Project
            && plan
                .project
                .as_ref()
                .is_some_and(|project| project.is_home && !project.is_help_home)
    };
    if let Some(home) = plans.iter().position(is_home_plan) {
        for index in 0..plans.len() {
            if index == home {
                continue;
            }
            if let Some(at) = plans[index].rows.iter().position(|row| row.row.is_chief) {
                let source_group_id = plans[index].group_id.clone();
                let row = plans[index].rows.remove(at);
                if focus.active_group_id.as_deref() == Some(source_group_id.as_str())
                    && focus.focused_session_id.as_deref()
                        == row.row.key.as_ref().map(|key| key.session_id.as_str())
                {
                    focus.active_group_id = Some(plans[home].group_id.clone());
                }
                plans[home].rows.insert(0, row);
                break;
            }
        }
    }
    for index in 0..plans.len() {
        let plan = &plans[index];
        if plan.kind != super::groups::GroupKind::Project || plan.rows.is_empty() {
            continue;
        }
        let Some(project) = plan.project.as_ref() else {
            continue;
        };
        let lifts_help = project.is_help_home && !project.is_home;
        let lifts_autopilot = project.is_autopilot;
        if !lifts_help && !lifts_autopilot {
            continue;
        }
        let machine = plan
            .remote_machine
            .as_ref()
            .map(|remote| remote.machine_id.clone());
        let Some(target) = plans.iter().position(|candidate| {
            let matches_home = if lifts_autopilot {
                is_real_home_plan(candidate)
            } else {
                is_home_plan(candidate)
            };
            matches_home
                && candidate
                    .remote_machine
                    .as_ref()
                    .map(|remote| remote.machine_id.clone())
                    == machine
        }) else {
            continue;
        };
        let source_group_id = plans[index].group_id.clone();
        let target_group_id = plans[target].group_id.clone();
        if focus.active_group_id.as_deref() == Some(source_group_id.as_str()) {
            focus.active_group_id = Some(target_group_id);
        }
        let rows = std::mem::take(&mut plans[index].rows);
        plans[target].rows.extend(rows);
    }
}

/// Fold key for Main Hub's per-computer repeated rows (`"chief"` or `"autopilot"`).
pub(crate) fn machine_fold_key(storage_id: &str, kind: &str) -> String {
    format!("{storage_id}:{kind}")
}

/// Folds other computers' "Ask Ghostex AI" and "Autopilot supervisor" rows under this computer's
/// row in Main Hub, marking the lead row with `MachineFold::Lead` (drawn as a neutral `+N` chip)
/// and each other computer's row with `MachineFold::Child` (hidden via `nesting.folded` while the
/// fold is closed, so section and group counts still include them).
///
/// CDXC:Chief 2026-11-24 DECISION:
/// User picked Part 1 option A: with computers combined, show only this computer's Ask Ghostex AI
/// row with a small neutral `+N` chip that unfolds the other computers' rows under it (each showing
/// its computer icon and name), and the same for Autopilot supervisor.
pub(crate) fn fold_home_machine_rows(
    sessions: &mut Vec<SessionView>,
    section_by_session: &mut Vec<SectionId>,
    is_home: bool,
    storage_id: &str,
    expanded_machine_folds: &BTreeSet<String>,
) {
    if !is_home {
        return;
    }
    fold_machine_kind(
        sessions,
        section_by_session,
        SectionId::Pinned,
        machine_fold_key(storage_id, "chief"),
        expanded_machine_folds,
        |row| row.is_chief,
    );
    fold_machine_kind(
        sessions,
        section_by_session,
        SectionId::Autopilot,
        machine_fold_key(storage_id, "autopilot"),
        expanded_machine_folds,
        |row| row.is_autopilot_supervisor,
    );
}

fn subtree_end(sessions: &[SessionView], root_idx: usize) -> usize {
    let mut end = root_idx + 1;
    while end < sessions.len() && sessions[end].nesting.depth > 0 {
        end += 1;
    }
    end
}

fn fold_machine_kind(
    sessions: &mut Vec<SessionView>,
    section_by_session: &mut Vec<SectionId>,
    section: SectionId,
    fold_key: String,
    expanded_machine_folds: &BTreeSet<String>,
    matches: impl Fn(&SessionRow) -> bool,
) {
    let indices: Vec<usize> = sessions
        .iter()
        .enumerate()
        .filter(|(index, session)| {
            section_by_session[*index] == section
                && session.nesting.depth == 0
                && matches(&session.row)
        })
        .map(|(index, _)| index)
        .collect();
    if indices.len() < 2 {
        return;
    }
    let machine_id_of = |row: &SessionRow| -> String {
        row.key
            .as_ref()
            .and_then(|key| key.machine.remote_id())
            .unwrap_or(LOCAL_MACHINE_ID)
            .to_string()
    };
    let lead_pos = indices
        .iter()
        .position(|&index| machine_id_of(&sessions[index].row) == LOCAL_MACHINE_ID)
        .unwrap_or(0);
    let lead_idx = indices[lead_pos];
    let lead_machine = machine_id_of(&sessions[lead_idx].row);
    let other_indices: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&index| index != lead_idx && machine_id_of(&sessions[index].row) != lead_machine)
        .collect();
    if other_indices.is_empty() {
        return;
    }
    let expanded = expanded_machine_folds.contains(&fold_key);
    let mut contains_active = false;
    for &child_idx in &other_indices {
        let machine_id = machine_id_of(&sessions[child_idx].row);
        let machine_label = sessions[child_idx]
            .row
            .machine_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                if machine_id == LOCAL_MACHINE_ID {
                    "Local".to_string()
                } else {
                    machine_id.clone()
                }
            });
        sessions[child_idx].machine_fold = Some(MachineFold::Child {
            fold_key: fold_key.clone(),
            machine_label,
            machine_id,
        });
        if !expanded {
            let end = subtree_end(sessions, child_idx);
            sessions[child_idx].nesting.folded = true;
            if sessions[child_idx].is_focused || sessions[child_idx].nesting.contains_active_session
            {
                contains_active = true;
            }
            for sub in (child_idx + 1)..end {
                sessions[sub].nesting.folded = true;
                if sessions[sub].is_focused {
                    contains_active = true;
                }
            }
        }
    }
    sessions[lead_idx].machine_fold = Some(MachineFold::Lead {
        key: fold_key,
        other_count: other_indices.len(),
        expanded,
        contains_active,
    });

    // Place the lead row first and keep every unfolded computer's row directly beneath it.
    let anchor = indices[0];
    let mut moved = vec![false; sessions.len()];
    let mut order: Vec<usize> = Vec::with_capacity(sessions.len());
    order.extend(0..anchor);
    let lead_end = subtree_end(sessions, lead_idx);
    for idx in lead_idx..lead_end {
        moved[idx] = true;
        order.push(idx);
    }
    for &child_idx in &other_indices {
        let end = subtree_end(sessions, child_idx);
        for idx in child_idx..end {
            moved[idx] = true;
            order.push(idx);
        }
    }
    for idx in anchor..sessions.len() {
        if !moved[idx] {
            order.push(idx);
        }
    }
    let old_sessions: Vec<Option<SessionView>> = sessions.drain(..).map(Some).collect();
    let old_sections: Vec<SectionId> = std::mem::take(section_by_session);
    let mut old_sessions = old_sessions;
    for idx in order {
        if let Some(session) = old_sessions[idx].take() {
            sessions.push(session);
            section_by_session.push(old_sections[idx]);
        }
    }
}
