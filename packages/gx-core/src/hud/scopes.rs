//! What view scopes read off the HUD: the Spaces the ACTIVE project belongs to
//! (`activeProjectSpaceRefs`), every Space a scope can name (`projectViewSpaces`), and every
//! project the sidebar lists (`projectViewProjects`)
//! (the app runtime's view-scopes helper, deleted, and
//! packages/shared/ghostex-settings/project-views.ts `projectViewSpaceOptions`).
//!
//! CDXC:Extensions 2026-09-20 WHY:
//! A view with a per-space override has to answer "is the ACTIVE project in this space?" while the
//! native work area header renders, and the answer lives in the owning daemon's collections and
//! spaces documents. It is resolved once here, beside the HUD that already carries
//! `projectViewSpaces`: ruling 1A, a space means the project's OWN space by membership, including
//! group and worktree-parent inheritance, exactly as the sidebar resolves it, so a project only the
//! built-in Other space holds resolves into no space at all.
//!
//! The documents are read from the store's side state, which for this computer is the document the
//! app owns and edits; the runtime read the copy its last snapshot carried.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::keys::{MachineId, ProjectKey};
use crate::presentation_store::{MachinePresentation, PresentationStore};

/// `stringFromRecord(worktree, 'parentProjectId')`.
fn parent_project_id(worktree: Option<&Value>) -> Option<String> {
    let parent = worktree?.get("parentProjectId")?.as_str()?.trim();
    (!parent.is_empty()).then(|| parent.to_string())
}

/// `createGpuiActiveProjectSpaceRefs`. The refs come out in space-id order.
pub(crate) fn active_project_space_refs(
    store: &PresentationStore,
    active_project_id: Option<&str>,
) -> Value {
    let refs = (|| {
        let active = ProjectKey::parse_workspace_project_id(active_project_id?)?;
        let section_key = match &active.machine {
            MachineId::Local => "local".to_string(),
            MachineId::Remote(machine_id) => format!("remote:{machine_id}"),
        };
        let machine = store.machine(&active.machine)?;
        // A remote machine counts only while its stream is live, as the runtime's map did.
        let loaded = match &active.machine {
            MachineId::Local => machine.loaded()?,
            MachineId::Remote(_) => store.loaded_live(&active.machine)?,
        };
        let spaces = machine.side_state().spaces.as_ref()?;
        let project_id = active.project_id.as_str();
        let domain_worktree = match &active.machine {
            MachineId::Local => machine
                .domain_project(project_id)
                .and_then(|project| project.get("worktree")),
            MachineId::Remote(_) => None,
        };
        let parent = parent_project_id(domain_worktree).or_else(|| {
            parent_project_id(
                loaded
                    .project(project_id)
                    .and_then(|project| project.worktree.as_ref()),
            )
        });
        let member = parent.as_deref().unwrap_or(project_id);
        let collection_id =
            machine
                .side_state()
                .project_collections
                .as_ref()
                .and_then(|collections| {
                    collections
                        .collections
                        .iter()
                        .find(|(_, collection)| {
                            collection.project_ids.iter().any(|id| id == member)
                        })
                        .map(|(collection_id, _)| collection_id.clone())
                });
        Some(
            spaces
                .spaces
                .iter()
                .filter(|(_, space)| match &collection_id {
                    Some(collection_id) => space.member_collection_ids.contains(collection_id),
                    None => space.member_project_ids.iter().any(|id| id == member),
                })
                .map(|(space_id, _)| {
                    let mut reference = Map::new();
                    reference.insert("sectionKey".into(), Value::from(section_key.clone()));
                    reference.insert("spaceId".into(), Value::from(space_id.as_str()));
                    Value::Object(reference)
                })
                .collect::<Vec<_>>(),
        )
    })();
    Value::Array(refs.unwrap_or_default())
}

/// `projectViewSpaceOptions` for one machine: its Spaces in their order, named, and for a remote
/// machine suffixed with the computer's name.
fn space_options(
    machine: &MachinePresentation,
    section_key: &str,
    computer_name: Option<&str>,
    out: &mut Vec<Value>,
) {
    let Some(spaces) = machine.side_state().spaces.as_ref() else {
        return;
    };
    for space_id in &spaces.order {
        let Some(space) = spaces.spaces.get(space_id) else {
            continue;
        };
        let name: String = space.name.trim().chars().take(256).collect();
        if name.is_empty() {
            continue;
        }
        let mut option = Map::new();
        option.insert("sectionKey".into(), Value::from(section_key));
        option.insert("spaceId".into(), Value::from(space_id.as_str()));
        option.insert(
            "name".into(),
            Value::from(match computer_name {
                Some(computer) => format!("{name} ({computer})"),
                None => name,
            }),
        );
        out.push(Value::Object(option));
    }
}

/// `projectViewSpaces`: this computer's Spaces, then each live remote machine's, named after the
/// computer as Settings saves it, else by its id.
pub(crate) fn project_view_spaces(
    store: &PresentationStore,
    machine_names: &BTreeMap<String, String>,
) -> Value {
    let mut out = Vec::new();
    if let Some(local) = store
        .machine(&MachineId::Local)
        .filter(|machine| machine.loaded().is_some())
    {
        space_options(local, "local", None, &mut out);
    }
    for (machine_id, machine) in store.machines() {
        let Some(remote_id) = machine_id.remote_id() else {
            continue;
        };
        if store.loaded_live(machine_id).is_none() {
            continue;
        }
        let name = machine_names
            .get(remote_id)
            .map(String::as_str)
            .unwrap_or(remote_id);
        space_options(
            machine,
            &format!("remote:{remote_id}"),
            Some(name),
            &mut out,
        );
    }
    Value::Array(out)
}

/// `createGpuiProjectViewProjects`: every project the sidebar lists, in the sidebar's order
/// (worktrees under their parents), this computer first and then each saved remote machine that
/// has rows, live or last seen. Chat projects, parked Recent Projects and projects closed here are
/// not listed.
///
/// CDXC:Extensions 2026-09-18 DECISION:
/// User: the view scope's Projects grid lists only the projects currently in the sidebar.
/// The sidebar rows are the authority, so parked Recent Projects, the synthetic Chats collection and
/// browser groups are all absent, and worktrees appear as their own rows exactly as the sidebar
/// shows them. Ids are the same machine-scoped ids the active project reports, so a ticked remote
/// project matches the project the titlebar is actually rendering for.
pub(crate) fn project_view_projects(
    store: &PresentationStore,
    remote_machines: &[(String, String)],
    parked: &BTreeMap<MachineId, BTreeSet<String>>,
) -> Value {
    let empty = BTreeSet::new();
    let machines = std::iter::once(MachineId::Local).chain(
        remote_machines
            .iter()
            .map(|(machine_id, _)| MachineId::Remote(machine_id.clone())),
    );
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for machine_id in machines {
        let Some(machine) = store.machine(&machine_id) else {
            continue;
        };
        let Some(loaded) = machine.loaded() else {
            continue;
        };
        let order = machine
            .side_state()
            .workspace_groups
            .as_ref()
            .map(|groups| groups.project_order.as_slice())
            .unwrap_or_default();
        let meta = crate::sidebar_view::projects::build_project_meta(
            machine,
            order,
            parked.get(&machine_id).unwrap_or(&empty),
        );
        for project_id in &meta.project_order {
            if machine.is_project_hidden(project_id) {
                continue;
            }
            let Some(project) = loaded.project(project_id) else {
                continue;
            };
            let name = project.title.trim();
            let scoped = ProjectKey {
                machine: machine_id.clone(),
                project_id: project_id.clone(),
            }
            .to_workspace_project_id();
            if name.is_empty() || !seen.insert(scoped.clone()) {
                continue;
            }
            let mut row = Map::new();
            row.insert("name".into(), Value::from(name));
            if let Some(path) = &project.path {
                row.insert("path".into(), Value::from(path.as_str()));
            }
            row.insert("projectId".into(), Value::from(scoped));
            out.push(Value::Object(row));
        }
    }
    Value::Array(out)
}
