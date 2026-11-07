//! The Project sidebar as one list for every machine: a project that is on several machines is
//! one entry, with the sessions of every copy in it.
//!
//! CDXC:RemoteMachines 2026-11-05 DECISION:
//! User: "i wanna switch to showing the projects from all my machines without a machine picker basically as much as possible ... if same project is on both machines then should just have 1 entry in my sidebar for that project". Unless Settings › Sidebar's Separate by machine is on, this computer's list takes every connected machine's projects: a project matches another by git remote, then by folder name, with the Unified sidebar's rule (`unified_sidebar::repo`, Q1351); two clones of one repository on the same machine were one entry too (Q1352, replaced by the 2026-11-07 decision below); the sessions of every copy are mixed in one list, each one from another machine carrying that machine's name (Q1350); and the header's creating actions (New Terminal, Create Agent, the ⋯ menu's Add Worktree, History and the rest) go to the copy a session of the project last started in (Q1353). A worktree project and a bot are never merged: a worktree is its own entry under its parent, and bots stay in Bots. While Worktree sessions show in their project is on, a worktree's sessions are listed in its parent's entry instead, on every machine (CDXC:Worktrees 2026-11-06 in worktree_fold.rs).
//!
//! CDXC:RemoteMachines 2026-11-07 DECISION:
//! Issue #461 (one computer, 15 projects that are subfolders of one monorepo, in 6 Spaces): the merge only ever joins the same project seen on different computers. Two projects of one machine are never one entry, whatever their remote: subfolders of one repository and two clones of it each keep their own entry, their own Space and their own sessions (this replaces Q1352's "two clones on the same machine are one entry"). The key a project matches by is its repository and its folder inside that repository (the path below `gitRepositoryRootPath`), so `monorepo/trading` on one computer joins `monorepo/trading` on another and never `monorepo/` or `monorepo/product-x`. Keying by the remote alone put the Product Space's sessions in the Trading project's entry, so the Trading Space drew them and its Space switch restored one of them.
//!
//! CDXC:RemoteMachines 2026-11-05 WHY:
//! A copy merged into another entry gives it its sessions only: its user-made session groups are listed with the entry's own sessions rather than as groups, because a session group is drawn inside the project that owns its id (`owner_project_group_id`), and that project is not drawn. A project only another machine has keeps its session groups, exactly as that machine's tab drew them.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::change::ChangeSummary;
use crate::core::Core;
use crate::keys::{MachineId, ProjectKey, SessionKey};
use crate::presentation_store::PresentationStore;
use crate::unified_sidebar::repo::{merge_repo_identities, repo_identity, RepoIdentity};

use super::groups::{
    FoldedWorktreePlan, GroupKind, GroupPlan, PlanActionTarget, ProjectContextInput, RowRef,
};
use super::inputs::{SidebarInputs, LOCAL_MACHINE_ID};
use super::membership::{project_members, ProjectMembers};
use super::model::SidebarUpdateWork;
use super::projects::{build_project_meta, ProjectMeta};
use super::rows::{session_row, side_chat_needs, RowContext};
use super::view::{MachineChip, RemoteMachineView};
use super::worktree_fold::{mark_row, row_worktree, MarkedRows};
use super::{CollectionsState, SpacesState};

/// What the merge keeps between updates.
#[derive(Default)]
pub(super) struct MergeCache {
    machines: BTreeMap<MachineId, GuestMachine>,
    /// Every other machine's row in this list, kept until the store says that session moved.
    rows: BTreeMap<SessionKey, RowRef>,
    /// Those rows marked with their worktree, for a worktree folded into its parent's entry
    /// (`worktree_fold.rs`).
    worktree_rows: MarkedRows,
}

/// One other machine's projects as this list reads them.
struct GuestMachine {
    meta: Arc<ProjectMeta>,
    parked: BTreeSet<String>,
    membership: BTreeMap<String, Arc<ProjectMembers>>,
    contexts: BTreeMap<String, Arc<ProjectContextInput>>,
}

pub(super) struct MergeInput<'a> {
    pub(super) core: &'a Core,
    pub(super) inputs: &'a SidebarInputs,
    pub(super) changes: &'a ChangeSummary,
    pub(super) rows_all_dirty: bool,
    pub(super) context: &'a RowContext<'a>,
}

/// What the list needs from the merge besides the plans it rewrote.
#[derive(Default)]
pub(super) struct MergeOutcome {
    /// The group ids no longer drawn (another machine's copy and its session groups), each with
    /// the entry that now lists its sessions: the focus follows them there.
    pub(super) merged_into: BTreeMap<String, String>,
}

/// One copy of a project an entry lists.
struct Copy {
    machine: MachineId,
    project_id: String,
    group_id: String,
    project: Arc<ProjectContextInput>,
    remote_machine: Option<RemoteMachineView>,
}

/// One machine besides this computer whose projects join the list.
struct Guest {
    machine: MachineId,
    label: String,
    /// Its pill (`machine_short_names.rs`).
    chip: MachineChip,
    is_stale: bool,
}

/// Rewrites this computer's `plans` into the one list: merges the copies of one repository into
/// one entry and adds the projects only other machines have, after this computer's own.
pub(super) fn merge_machines(
    previous: Option<MergeCache>,
    input: &MergeInput<'_>,
    plans: &mut Vec<GroupPlan>,
    next_row_id: &mut u64,
    work: &mut SidebarUpdateWork,
) -> (MergeCache, MergeOutcome) {
    let store = input.core.presentation();
    let changes = input.changes;
    let mut previous = previous.unwrap_or_default();
    let mut cache = MergeCache {
        machines: BTreeMap::new(),
        rows: if input.rows_all_dirty {
            BTreeMap::new()
        } else {
            std::mem::take(&mut previous.rows)
        },
        worktree_rows: MarkedRows::new(),
    };
    let mut previous_marked = std::mem::take(&mut previous.worktree_rows);
    let fold_worktrees = input.inputs.settings.worktree_sessions_in_project
        && !input.inputs.settings.unified_sidebar;
    for key in changes
        .sessions_changed
        .iter()
        .chain(&changes.sessions_removed)
    {
        cache.rows.remove(key);
        // A side chat's questions and approvals show on its main session's row.
        if let Some(parent) = store
            .machine(&key.machine)
            .and_then(|entry| entry.effective_session(&key.project_id, &key.session_id))
            .and_then(|session| session.side_chat_parent_session_id.clone())
        {
            cache.rows.remove(&SessionKey {
                machine: key.machine.clone(),
                project_id: key.project_id.clone(),
                session_id: parent,
            });
        }
    }
    cache
        .rows
        .retain(|key, _| !changes.machines_reloaded.contains(&key.machine));

    let guests = guests(store, input.inputs);
    for guest in &guests {
        let machine = refresh_guest(
            previous.machines.remove(&guest.machine),
            store,
            input.inputs,
            changes,
            &guest.machine,
        );
        cache.machines.insert(guest.machine.clone(), machine);
    }

    // Every repository the list holds, so one spelled two ways (an SSH alias, no origin yet) is
    // still one entry (`merge_repo_identities`).
    let mut identities: Vec<RepoIdentity> = Vec::new();
    for plan in plans.iter() {
        if let Some(project) = mergeable(plan.kind, plan.project.as_deref()) {
            identities.push(identity_of(project));
        }
    }
    for guest in &guests {
        let machine = &cache.machines[&guest.machine];
        for project in machine.contexts.values() {
            if let Some(project) = mergeable(GroupKind::Project, Some(project)) {
                identities.push(identity_of(project));
            }
        }
    }
    let canonical = merge_repo_identities(identities.iter());
    let key_of = |project: &ProjectContextInput| -> String {
        let identity = identity_of(project);
        let from_origin = !identity.key.starts_with("dir:");
        let key = canonical
            .get(&identity.key)
            .map_or(identity.key, |merged| merged.key.clone());
        // A folder with no origin is keyed by its own folder name already.
        match folder_in_repository(project).filter(|_| from_origin) {
            Some(folder) => format!("{key}#{folder}"),
            None => key,
        }
    };

    let mut outcome = MergeOutcome::default();
    let mut drawn_keys: HashSet<SessionKey> = plans
        .iter()
        .flat_map(|plan| plan.rows.iter())
        .filter_map(|row| row.row.key.clone())
        .collect();
    // The entries each project key is drawn as, by their index in `plans`, in list order. A key
    // has several when one machine has that project more than once (two clones of a repository),
    // and each of those stays its own entry (`entry_for`).
    let mut entries_of_key: HashMap<String, Vec<usize>> = HashMap::new();
    // The entry each copy is listed in, for a worktree to find its parent's.
    let mut entry_of_copy: HashMap<ProjectKey, usize> = HashMap::new();
    let mut copies: BTreeMap<usize, Vec<Copy>> = BTreeMap::new();
    let mut names: BTreeMap<usize, Vec<MachineChip>> = BTreeMap::new();
    let local_chip = machine_chip(input.inputs, LOCAL_MACHINE_ID);

    // This computer's projects first, in its own order, each its own entry: two projects of one
    // machine are never merged, whatever remote they share.
    let mut index = 0;
    while index < plans.len() {
        let Some(project) = mergeable(plans[index].kind, plans[index].project.as_deref()) else {
            index += 1;
            continue;
        };
        let project = Arc::new(project.clone());
        let key = key_of(&project);
        let subgroups = subgroups_after(plans, index);
        entry_of_copy.insert(
            ProjectKey {
                machine: MachineId::Local,
                project_id: project.project_id.clone(),
            },
            index,
        );
        let copy = Copy {
            machine: MachineId::Local,
            project_id: project.project_id.clone(),
            group_id: plans[index].group_id.clone(),
            project,
            remote_machine: None,
        };
        entries_of_key.entry(key).or_default().push(index);
        copies.insert(index, vec![copy]);
        names.insert(index, vec![local_chip.clone()]);
        index = subgroups.end;
    }

    // Then every other machine's, in the machine tabs' order.
    for guest in &guests {
        let machine = &cache.machines[&guest.machine];
        let needs = side_chat_needs(store, &guest.machine);
        let context = RowContext {
            side_chat_needs: &needs,
            ..*input.context
        };
        for project_id in machine.meta.project_order.iter() {
            let (Some(project), Some(members)) = (
                machine.contexts.get(project_id),
                machine.membership.get(project_id),
            ) else {
                continue;
            };
            if project.bot_profile.is_some() {
                continue;
            }
            let project_key = ProjectKey {
                machine: guest.machine.clone(),
                project_id: project_id.clone(),
            };
            let group_id = project_key.to_sidebar_group_id();
            let remote_machine = RemoteMachineView {
                machine_id: guest
                    .machine
                    .remote_id()
                    .unwrap_or(LOCAL_MACHINE_ID)
                    .to_string(),
                machine_name: guest.label.clone(),
                project_id: Some(project_id.clone()),
            };
            let mut row_of = |session_id: &str, drawn_keys: &mut HashSet<SessionKey>| {
                let key = SessionKey {
                    machine: guest.machine.clone(),
                    project_id: project_id.clone(),
                    session_id: session_id.to_string(),
                };
                // A coordinator's thread is already drawn under its coordinator.
                if !drawn_keys.insert(key.clone()) {
                    return None;
                }
                guest_row(
                    &mut cache.rows,
                    store,
                    key,
                    &guest.chip,
                    input.inputs,
                    &context,
                    next_row_id,
                    work,
                )
            };
            // A worktree whose parent has an entry is listed in it (`worktree_fold.rs`).
            let parent_entry = project
                .worktree
                .as_ref()
                .filter(|_| fold_worktrees)
                .and_then(|worktree| {
                    let parent = ProjectKey {
                        machine: guest.machine.clone(),
                        project_id: worktree.parent_project_id.clone(),
                    };
                    entry_of_copy.get(&parent).copied().or_else(|| {
                        machine
                            .contexts
                            .get(&worktree.parent_project_id)
                            .and_then(|parent| mergeable(GroupKind::Project, Some(parent)))
                            .and_then(|parent| {
                                entry_for(&entries_of_key, &copies, &key_of(parent), &guest.machine)
                            })
                    })
                });
            if let Some(entry) = parent_entry {
                let entry_id = plans[entry].group_id.clone();
                let worktree = row_worktree(&group_id, project);
                let session_ids = members.session_ids.iter().chain(
                    members
                        .subgroups
                        .iter()
                        .flat_map(|subgroup| subgroup.session_ids.iter()),
                );
                for session_id in session_ids {
                    if let Some(row) = row_of(session_id, &mut drawn_keys) {
                        let row = mark_row(
                            row,
                            &worktree,
                            &mut previous_marked,
                            &mut cache.worktree_rows,
                        );
                        plans[entry].rows.push(row);
                    }
                }
                outcome
                    .merged_into
                    .insert(group_id.clone(), entry_id.clone());
                for subgroup in &members.subgroups {
                    outcome
                        .merged_into
                        .insert(subgroup.sidebar_group_id.clone(), entry_id.clone());
                }
                plans[entry].folded_worktrees.push(FoldedWorktreePlan {
                    group_id,
                    project: project.clone(),
                    remote_machine: Some(remote_machine),
                });
                continue;
            }
            let entry = mergeable(GroupKind::Project, Some(project)).and_then(|project| {
                entry_for(&entries_of_key, &copies, &key_of(project), &guest.machine)
            });
            let copy = Copy {
                machine: guest.machine.clone(),
                project_id: project_id.clone(),
                group_id: group_id.clone(),
                project: project.clone(),
                remote_machine: Some(remote_machine.clone()),
            };
            if let Some(entry) = entry {
                let entry_id = plans[entry].group_id.clone();
                let session_ids = members.session_ids.iter().chain(
                    members
                        .subgroups
                        .iter()
                        .flat_map(|subgroup| subgroup.session_ids.iter()),
                );
                for session_id in session_ids {
                    if let Some(row) = row_of(session_id, &mut drawn_keys) {
                        plans[entry].rows.push(row);
                    }
                }
                outcome.merged_into.insert(group_id, entry_id.clone());
                for subgroup in &members.subgroups {
                    outcome
                        .merged_into
                        .insert(subgroup.sidebar_group_id.clone(), entry_id.clone());
                }
                copies.entry(entry).or_default().push(copy);
                entry_of_copy.insert(project_key.clone(), entry);
                let entry_names = names.entry(entry).or_default();
                if !entry_names.contains(&guest.chip) {
                    entry_names.push(guest.chip.clone());
                }
                continue;
            }
            // A project only this machine has (so far): its own entry, as its tab draws it.
            let rows: Vec<RowRef> = members
                .session_ids
                .iter()
                .filter_map(|session_id| row_of(session_id, &mut drawn_keys))
                .collect();
            let entry = plans.len();
            plans.push(GroupPlan {
                group_id,
                storage_id: super::model::storage_id(
                    &guest.machine,
                    &project_key.to_workspace_project_id(),
                ),
                title: project.title.clone(),
                kind: GroupKind::Project,
                rows,
                project: Some(project.clone()),
                remote_machine: Some(remote_machine.clone()),
                is_stale: guest.is_stale,
                machine_names: Vec::new(),
                action_target: None,
                folded_worktrees: Vec::new(),
            });
            for subgroup in &members.subgroups {
                let rows: Vec<RowRef> = subgroup
                    .session_ids
                    .iter()
                    .filter_map(|session_id| row_of(session_id, &mut drawn_keys))
                    .collect();
                plans.push(GroupPlan {
                    group_id: subgroup.sidebar_group_id.clone(),
                    storage_id: super::model::storage_id(
                        &guest.machine,
                        &subgroup.sidebar_group_id,
                    ),
                    title: subgroup.title.clone(),
                    kind: GroupKind::Subgroup,
                    rows,
                    project: None,
                    remote_machine: Some(remote_machine.clone()),
                    is_stale: guest.is_stale,
                    machine_names: Vec::new(),
                    action_target: None,
                    folded_worktrees: Vec::new(),
                });
            }
            if let Some(project) = mergeable(GroupKind::Project, Some(project)) {
                entries_of_key
                    .entry(key_of(project))
                    .or_default()
                    .push(entry);
            }
            entry_of_copy.insert(project_key, entry);
            copies.insert(entry, vec![copy]);
            names.insert(entry, vec![guest.chip.clone()]);
        }
    }

    for (entry, entry_names) in names {
        // A project only this computer has says nothing; one elsewhere names every machine.
        if entry_names.len() > 1 || plans[entry].remote_machine.is_some() {
            plans[entry].machine_names = entry_names;
        }
    }
    for (entry, entry_copies) in &copies {
        if entry_copies.len() > 1 {
            plans[*entry].action_target = last_used_copy(&plans[*entry], entry_copies);
        }
    }
    (cache, outcome)
}

/// The machines besides this computer whose projects the list takes: every machine tab the host
/// feeds and has loaded, never one that is this computer under another name.
fn guests(store: &PresentationStore, inputs: &SidebarInputs) -> Vec<Guest> {
    inputs
        .host
        .machines
        .iter()
        .filter(|tab| !tab.is_local() && tab.fed && !tab.this_computer)
        .filter_map(|tab| {
            let machine = MachineId::Remote(tab.machine_id.clone());
            let entry = store.machine(&machine)?;
            entry.loaded()?;
            let label = machine_label(inputs, &tab.machine_id);
            Some(Guest {
                chip: machine_chip(inputs, &tab.machine_id),
                label,
                // A machine whose stream is down keeps drawing its last rows, faded
                // (`SidebarViewModel::update`).
                is_stale: entry.connection().phase != crate::ConnectionPhase::Live,
                machine,
            })
        })
        .collect()
}

/// A machine's pill: its short name as its tab carries it (the host makes it, gx-core
/// `machine_short_names`), and its name.
fn machine_chip(inputs: &SidebarInputs, machine_id: &str) -> MachineChip {
    let name = machine_label(inputs, machine_id);
    let short_name = inputs
        .host
        .machine(machine_id)
        .map(|tab| tab.short_name.trim().to_string())
        .filter(|short| !short.is_empty())
        .unwrap_or_else(|| {
            crate::machine_short_names::machine_short_names(&[
                crate::machine_short_names::MachineNameInput {
                    name: name.clone(),
                    ..Default::default()
                },
            ])
            .remove(0)
        });
    MachineChip { name, short_name }
}

/// A machine's name as its tab spells it.
fn machine_label(inputs: &SidebarInputs, machine_id: &str) -> String {
    inputs
        .host
        .machine(machine_id)
        .map(|tab| tab.label.trim())
        .filter(|label| !label.is_empty())
        .unwrap_or(if machine_id == LOCAL_MACHINE_ID {
            "This computer"
        } else {
            machine_id
        })
        .to_string()
}

/// Brings one other machine's projects up to date, keeping what nothing touched.
fn refresh_guest(
    previous: Option<GuestMachine>,
    store: &PresentationStore,
    inputs: &SidebarInputs,
    changes: &ChangeSummary,
    machine: &MachineId,
) -> GuestMachine {
    let parked = inputs.host.parked_project_ids(machine);
    let ours = |project: &ProjectKey| project.machine == *machine;
    let meta_dirty = previous
        .as_ref()
        .is_none_or(|previous| previous.parked != *parked)
        || changes.machines_reloaded.contains(machine)
        || changes.project_order_changed.contains(machine)
        || changes.projects_changed.iter().any(ours)
        || changes.projects_removed.iter().any(ours)
        || changes.side_state.workspace_groups;
    let previous = previous.filter(|_| !meta_dirty);
    let Some(entry) = store.machine(machine) else {
        return GuestMachine {
            meta: Arc::new(ProjectMeta::default()),
            parked: parked.clone(),
            membership: BTreeMap::new(),
            contexts: BTreeMap::new(),
        };
    };
    let meta = match &previous {
        Some(previous) => previous.meta.clone(),
        // The machine's own order document, as its tab reads it.
        None => Arc::new(build_project_meta(
            entry,
            entry
                .side_state()
                .workspace_groups
                .as_ref()
                .map(|groups| groups.project_order.as_slice())
                .unwrap_or_default(),
            parked,
        )),
    };
    let dirty_projects: BTreeSet<&str> = changes
        .session_order_changed
        .iter()
        .filter(|project| ours(project))
        .map(|project| project.project_id.as_str())
        .chain(
            changes
                .sessions_changed
                .iter()
                .chain(&changes.sessions_removed)
                .filter(|session| session.machine == *machine)
                .map(|session| session.project_id.as_str()),
        )
        .collect();
    let mut membership = BTreeMap::new();
    let mut contexts = BTreeMap::new();
    for project_id in &meta.project_order {
        let members = previous
            .as_ref()
            .filter(|_| !dirty_projects.contains(project_id.as_str()))
            .and_then(|previous| previous.membership.get(project_id).cloned())
            .unwrap_or_else(|| Arc::new(project_members(store, entry, machine, project_id)));
        membership.insert(project_id.clone(), members);
        let context = previous
            .as_ref()
            .and_then(|previous| previous.contexts.get(project_id).cloned())
            .or_else(|| {
                super::model::project_context(store, machine, &meta, project_id, inputs).map(
                    |mut project| {
                        // The git numbers are this computer's, by its own project ids.
                        project.diff_stats = Default::default();
                        Arc::new(project)
                    },
                )
            });
        if let Some(context) = context {
            contexts.insert(project_id.clone(), context);
        }
    }
    GuestMachine {
        meta,
        parked: parked.clone(),
        membership,
        contexts,
    }
}

/// The project a plan stands for, when it may share an entry with another copy: a project group
/// that is neither a bot nor a worktree.
fn mergeable(
    kind: GroupKind,
    project: Option<&ProjectContextInput>,
) -> Option<&ProjectContextInput> {
    project.filter(|project| {
        kind == GroupKind::Project && project.bot_profile.is_none() && project.worktree.is_none()
    })
}

fn identity_of(project: &ProjectContextInput) -> RepoIdentity {
    repo_identity(
        project.git_remote_origin_url.as_deref(),
        &project.path,
        None,
        &project.title,
    )
}

/// The entry a copy on `machine` joins: the first drawn for its key that holds no copy from that
/// machine yet, so only copies on different machines share one, and a machine's second clone of a
/// repository joins the other machine's second clone, if it has one.
fn entry_for(
    entries_of_key: &HashMap<String, Vec<usize>>,
    copies: &BTreeMap<usize, Vec<Copy>>,
    key: &str,
    machine: &MachineId,
) -> Option<usize> {
    entries_of_key.get(key)?.iter().copied().find(|entry| {
        copies
            .get(entry)
            .is_none_or(|copies| copies.iter().all(|copy| copy.machine != *machine))
    })
}

/// Where a project's folder is inside its repository (`product-x`, `company/sub-co`), lowercase,
/// when it is a subfolder rather than the repository itself. `None` for the repository's root and
/// when the root is unknown or does not contain the folder.
fn folder_in_repository(project: &ProjectContextInput) -> Option<String> {
    let normalize = |path: &str| {
        path.trim()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    };
    let root = normalize(project.git_repository_root_path.as_deref()?);
    let path = normalize(&project.path);
    let folder = path
        .strip_prefix(&root)?
        .strip_prefix('/')?
        .trim_matches('/');
    (!folder.is_empty()).then(|| folder.to_lowercase())
}

/// The session groups drawn right after the project plan at `index`, as indices.
fn subgroups_after(plans: &[GroupPlan], index: usize) -> std::ops::Range<usize> {
    let end = plans[index + 1..]
        .iter()
        .position(|plan| plan.kind != GroupKind::Subgroup)
        .map_or(plans.len(), |offset| index + 1 + offset);
    index + 1..end
}

/// The copy the newest session of an entry started in, when that is not the entry's own.
fn last_used_copy(plan: &GroupPlan, copies: &[Copy]) -> Option<PlanActionTarget> {
    let newest = plan
        .rows
        .iter()
        .filter_map(|row| {
            let key = row.row.key.as_ref()?;
            Some((row.row.timing.created_ms.unwrap_or(0), key))
        })
        .max_by_key(|(created_ms, _)| *created_ms)?
        .1;
    let copy = copies
        .iter()
        .find(|copy| copy.machine == newest.machine && copy.project_id == newest.project_id)?;
    (copy.group_id != plan.group_id).then(|| PlanActionTarget {
        group_id: copy.group_id.clone(),
        project: copy.project.clone(),
        remote_machine: copy.remote_machine.clone(),
    })
}

/// Another machine's session row: the one already held when nothing touched it.
#[allow(clippy::too_many_arguments)]
fn guest_row(
    rows: &mut BTreeMap<SessionKey, RowRef>,
    store: &PresentationStore,
    key: SessionKey,
    machine: &MachineChip,
    inputs: &SidebarInputs,
    context: &RowContext<'_>,
    next_row_id: &mut u64,
    work: &mut SidebarUpdateWork,
) -> Option<RowRef> {
    if let Some(row) = rows.get(&key).filter(|row| {
        row.row.machine_name.as_deref() == Some(machine.name.as_str())
            && row.row.machine_short_name.as_deref() == Some(machine.short_name.as_str())
    }) {
        return Some(row.clone());
    }
    let session = store
        .machine(&key.machine)?
        .effective_session(&key.project_id, &key.session_id)?;
    let sidebar_id = key.to_sidebar_session_id();
    let mut row = session_row(
        key.clone(),
        &session,
        super::inputs::CloseAfterDoneInput::from_session(&session).as_ref(),
        inputs.host.local_delayed_sends.get(&sidebar_id),
        context,
    );
    row.machine_name = Some(machine.name.clone());
    row.machine_short_name = Some(machine.short_name.clone());
    *next_row_id += 1;
    work.rows_built += 1;
    let row = RowRef {
        id: *next_row_id,
        row: Arc::new(row),
    };
    rows.insert(key, row.clone());
    Some(row)
}

/// This computer's Spaces and groups with every other computer's added (by id, else by name), so a
/// project only another computer has is in its Space and its group here too. The documents are
/// kept alike (`project_docs::shared`); this reads them before a pass has, and holds the other
/// computers' own project ids, which only their projects' entries match.
pub(super) fn union_documents(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    spaces: Option<SpacesState>,
    mut collections: CollectionsState,
) -> (Option<SpacesState>, CollectionsState) {
    let mut spaces = spaces;
    for guest in guests(store, inputs) {
        let Some(side) = store
            .machine(&guest.machine)
            .map(|entry| entry.side_state())
        else {
            continue;
        };
        if let Some(theirs) = side.project_collections.as_ref() {
            let mut renamed: BTreeMap<String, String> = BTreeMap::new();
            for collection in CollectionsState::from_wire(theirs).collections {
                let key = collection.title.trim().to_lowercase();
                let found = collections.collections.iter_mut().find(|ours| {
                    ours.collection_id == collection.collection_id
                        || (!key.is_empty() && ours.title.trim().to_lowercase() == key)
                });
                match found {
                    Some(ours) => {
                        renamed
                            .insert(collection.collection_id.clone(), ours.collection_id.clone());
                        for project_id in collection.project_ids {
                            if !ours.project_ids.contains(&project_id) {
                                ours.project_ids.push(project_id);
                            }
                        }
                    }
                    None => collections.collections.push(collection),
                }
            }
            if let (Some(ours), Some(theirs)) = (spaces.as_mut(), side.spaces.as_ref()) {
                union_spaces(ours, &SpacesState::from_wire(theirs), &renamed);
            }
        } else if let (Some(ours), Some(theirs)) = (spaces.as_mut(), side.spaces.as_ref()) {
            union_spaces(ours, &SpacesState::from_wire(theirs), &BTreeMap::new());
        }
    }
    (spaces, collections)
}

/// [`union_documents`]' Spaces alone, with this computer's groups to read the others' through.
pub(crate) fn union_spaces_of(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    spaces: SpacesState,
) -> SpacesState {
    let collections = store
        .machine(&MachineId::Local)
        .and_then(|entry| entry.side_state().project_collections.as_ref())
        .map(CollectionsState::from_wire)
        .unwrap_or_default();
    union_documents(store, inputs, Some(spaces.clone()), collections)
        .0
        .unwrap_or(spaces)
}

/// `theirs` folded into `ours`, its group ids read through `renamed` (their id → ours).
fn union_spaces(ours: &mut SpacesState, theirs: &SpacesState, renamed: &BTreeMap<String, String>) {
    for space in theirs.ordered() {
        let key = space.name.trim().to_lowercase();
        let found = ours.order.iter().find(|id| {
            ours.spaces.get(*id).is_some_and(|ours| {
                ours.space_id == space.space_id
                    || (!key.is_empty() && ours.name.trim().to_lowercase() == key)
            })
        });
        let id = match found {
            Some(id) => id.clone(),
            None => {
                let mut copy = space.clone();
                copy.member_collection_ids.clear();
                copy.member_project_ids.clear();
                ours.order.push(copy.space_id.clone());
                ours.spaces.insert(copy.space_id.clone(), copy);
                space.space_id.clone()
            }
        };
        let Some(target) = ours.spaces.get_mut(&id) else {
            continue;
        };
        for collection_id in &space.member_collection_ids {
            let collection_id = renamed.get(collection_id).unwrap_or(collection_id);
            if !target.member_collection_ids.contains(collection_id) {
                target.member_collection_ids.push(collection_id.clone());
            }
        }
        for project_id in &space.member_project_ids {
            if !target.member_project_ids.contains(project_id) {
                target.member_project_ids.push(project_id.clone());
            }
        }
    }
}
