//! One Spaces list and one set of project groups for every computer.
//!
//! CDXC:Spaces 2026-11-05 DECISION:
//! User: "we need to rethink spaces", then Q1354 "merge today's Spaces by name" (with a toast saying to name Spaces the same on each computer so they get grouped), Q1355 "kept on every computer" and Q1356 "each machine's tab uses the shared Spaces narrowed to that machine's projects". Every gxserver keeps its own Spaces and project-groups documents, and the desktop keeps them alike: the same Spaces and groups (same ids, names, icons, colours, order) on every computer, and a repository in the same Space or group on all of them, each computer listing its own copies. A Space or group is the same one on two computers when its id is, else when its name is (case and outer spaces aside). The computer whose document changed since the last pass decides first, so an edit made anywhere (this app, another app, a phone) reaches the others, a removal included; on a first pass nothing has changed yet and every computer's Spaces are merged.
//!
//! CDXC:Spaces 2026-11-05 WHY:
//! A member this pass cannot place (a hidden or parked project, a worktree, a bot, a collection the document names but does not hold) stays where its own computer has it: rewriting a document from what is on screen alone would drop it.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/shared_project_docs.rs (when it runs and where it writes), sidebar_view/merged.rs (the one list that reads the documents together).

use std::collections::{BTreeMap, BTreeSet};

use crate::keys::MachineId;
use crate::presentation_store::PresentationStore;
use crate::sidebar_view::{Collection, CollectionsState, Space, SpacesState};
use crate::unified_sidebar::repo::{merge_repo_identities, repo_identity, RepoIdentity};

use super::{CollectionsDocument, SpacesDocument};

/// One computer's documents and projects, as the pass reads them.
#[derive(Clone, Debug)]
pub struct SharedDocsMachine {
    pub machine: MachineId,
    /// `None` while the machine has published no Spaces document (an older gxserver).
    pub spaces: Option<SpacesDocument>,
    pub collections: CollectionsDocument,
    /// Project id → repository key, for the projects that share a Space with their copies (not a
    /// worktree, not a bot), in the machine's project order.
    pub repos: Vec<(String, String)>,
}

/// What a pass left a computer with, so the next pass can tell what changed there since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedDocsSnapshot {
    pub spaces: Option<SpacesState>,
    pub collections: CollectionsState,
}

impl SharedDocsSnapshot {
    pub fn of(machine: &SharedDocsMachine) -> Self {
        Self {
            spaces: machine.spaces.as_ref().map(|spaces| spaces.state.clone()),
            collections: machine.collections.state.clone(),
        }
    }
}

/// A computer's new documents; `None` where its own is already right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedDocsWrite {
    pub machine: MachineId,
    pub spaces: Option<SpacesDocument>,
    pub collections: Option<CollectionsDocument>,
}

/// What one pass decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedDocsPlan {
    pub writes: Vec<SharedDocsWrite>,
    /// What every computer holds after the writes, for the next pass.
    pub snapshots: BTreeMap<MachineId, SharedDocsSnapshot>,
    /// A first pass found a Space on one computer with no Space of the same name on another, so
    /// the toast says to name them alike.
    pub names_differ: bool,
    /// The groups this and earlier passes gave a computer a new id for: (computer, its old id) →
    /// the shared id. Its Spaces still name the old id until their own write lands.
    pub group_aliases: BTreeMap<(MachineId, String), String>,
}

/// Where a repository is filed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Placement {
    Collection(String),
    Space(String),
    Nowhere,
}

/// A Space or a group as every computer will hold it.
#[derive(Clone, Debug)]
struct SharedEntry {
    id: String,
    name: String,
    icon: String,
    color: String,
}

/// The same thing on several computers, matched by id, else by name.
#[derive(Default)]
struct Identities {
    entries: Vec<SharedEntry>,
    /// (machine, its own id) → shared id.
    of_machine: BTreeMap<(MachineId, String), String>,
}

impl Identities {
    fn adopt(&mut self, machine: &MachineId, own_id: &str, entry: SharedEntry) {
        let key = normalized(&entry.name);
        let shared = self
            .entries
            .iter()
            .find(|shared| shared.id == own_id)
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|shared| !key.is_empty() && normalized(&shared.name) == key)
            })
            .map(|shared| shared.id.clone());
        let shared = shared.unwrap_or_else(|| {
            self.entries.push(entry);
            own_id.to_string()
        });
        self.of_machine
            .insert((machine.clone(), own_id.to_string()), shared);
    }

    fn shared_id(&self, machine: &MachineId, own_id: &str) -> Option<&String> {
        self.of_machine.get(&(machine.clone(), own_id.to_string()))
    }

    fn entry(&self, id: &str) -> Option<&SharedEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
}

fn normalized(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Brings every computer's Spaces and groups into one shared set. `last` is what the previous pass
/// left each computer with; empty on the first pass of an app run.
pub fn plan_shared_docs(
    machines: &[SharedDocsMachine],
    last: &BTreeMap<MachineId, SharedDocsSnapshot>,
    group_aliases: &BTreeMap<(MachineId, String), String>,
    deleted_groups: &BTreeSet<String>,
) -> SharedDocsPlan {
    let first_pass = last.is_empty();
    // The computers that changed since the last pass decide first, this computer before the others.
    let changed = |machine: &SharedDocsMachine| {
        last.get(&machine.machine)
            .is_some_and(|snapshot| *snapshot != SharedDocsSnapshot::of(machine))
    };
    let mut ordered: Vec<(&SharedDocsMachine, bool)> = machines
        .iter()
        .filter(|machine| changed(machine))
        .map(|machine| (machine, true))
        .collect();
    ordered.extend(
        machines
            .iter()
            .filter(|machine| !changed(machine))
            .map(|machine| (machine, false)),
    );

    // A Space a deciding computer removed since the last pass stays removed everywhere. A group is
    // only removed everywhere when the user deleted it (`deleted_groups`, Ungroup): one that went
    // because its last project on one computer left it still holds the others' projects.
    let mut deleted_spaces: BTreeSet<String> = BTreeSet::new();
    let deleted_collections: &BTreeSet<String> = deleted_groups;
    for (machine, deciding) in &ordered {
        let Some(previous) = last.get(&machine.machine).filter(|_| *deciding) else {
            continue;
        };
        if let (Some(before), Some(now)) = (&previous.spaces, &machine.spaces) {
            deleted_spaces.extend(
                before
                    .order
                    .iter()
                    .filter(|id| !now.state.spaces.contains_key(*id))
                    .cloned(),
            );
        }
    }

    let mut spaces = Identities::default();
    let mut collections = Identities::default();
    for (machine, _) in &ordered {
        if let Some(document) = &machine.spaces {
            for space in document.state.ordered() {
                spaces.adopt(
                    &machine.machine,
                    &space.space_id,
                    SharedEntry {
                        id: space.space_id.clone(),
                        name: space.name.clone(),
                        icon: space.icon.clone(),
                        color: space.color.clone(),
                    },
                );
            }
        }
        for collection in &machine.collections.state.collections {
            collections.adopt(
                &machine.machine,
                &collection.collection_id,
                SharedEntry {
                    id: collection.collection_id.clone(),
                    name: collection.title.clone(),
                    icon: String::new(),
                    color: collection.color.clone(),
                },
            );
        }
    }
    // A deleted group's projects keep the group's Space, as an Ungroup on one computer does there
    // (`keep_spaces_across_collection_edit`).
    let mut deleted_group_space: BTreeMap<String, String> = BTreeMap::new();
    for (machine, _) in &ordered {
        let Some(document) = &machine.spaces else {
            continue;
        };
        for space in document.state.ordered() {
            for collection_id in &space.member_collection_ids {
                let group = collections
                    .shared_id(&machine.machine, collection_id)
                    .or_else(|| {
                        group_aliases.get(&(machine.machine.clone(), collection_id.clone()))
                    })
                    .cloned()
                    .unwrap_or_else(|| collection_id.clone());
                if deleted_collections.contains(&group) {
                    let space = spaces
                        .shared_id(&machine.machine, &space.space_id)
                        .cloned()
                        .unwrap_or_else(|| space.space_id.clone());
                    deleted_group_space.entry(group).or_insert(space);
                }
            }
        }
    }
    for (key, shared) in group_aliases {
        collections
            .of_machine
            .entry(key.clone())
            .or_insert_with(|| shared.clone());
    }
    spaces
        .entries
        .retain(|entry| !deleted_spaces.contains(&entry.id));
    collections
        .entries
        .retain(|entry| !deleted_collections.contains(&entry.id));
    let live_space = |id: Option<&String>| id.filter(|id| spaces.entry(id).is_some()).cloned();
    let live_collection =
        |id: Option<&String>| id.filter(|id| collections.entry(id).is_some()).cloned();

    // Where each repository and each group is filed: the first computer to say, a deciding one
    // saying "nowhere" too.
    let mut placement: BTreeMap<String, Placement> = BTreeMap::new();
    let mut group_space: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (machine, deciding) in &ordered {
        let repo_of: BTreeMap<&str, &str> = machine
            .repos
            .iter()
            .map(|(project, repo)| (project.as_str(), repo.as_str()))
            .collect();
        for collection in &machine.collections.state.collections {
            let Some(shared) =
                live_collection(collections.shared_id(&machine.machine, &collection.collection_id))
            else {
                let space = collections
                    .shared_id(&machine.machine, &collection.collection_id)
                    .and_then(|group| deleted_group_space.get(group))
                    .filter(|space| !deleted_spaces.contains(*space));
                if let Some(space) = space {
                    for project_id in &collection.project_ids {
                        if let Some(repo) = repo_of.get(project_id.as_str()) {
                            placement
                                .entry((*repo).to_string())
                                .or_insert_with(|| Placement::Space(space.clone()));
                        }
                    }
                }
                continue;
            };
            for project_id in &collection.project_ids {
                if let Some(repo) = repo_of.get(project_id.as_str()) {
                    placement
                        .entry((*repo).to_string())
                        .or_insert_with(|| Placement::Collection(shared.clone()));
                }
            }
        }
        if let Some(document) = &machine.spaces {
            for space in document.state.ordered() {
                let Some(shared) = live_space(spaces.shared_id(&machine.machine, &space.space_id))
                else {
                    continue;
                };
                for collection_id in &space.member_collection_ids {
                    if let Some(group) =
                        live_collection(collections.shared_id(&machine.machine, collection_id))
                    {
                        group_space
                            .entry(group)
                            .or_insert_with(|| Some(shared.clone()));
                    }
                }
                for project_id in &space.member_project_ids {
                    if let Some(repo) = repo_of.get(project_id.as_str()) {
                        placement
                            .entry((*repo).to_string())
                            .or_insert_with(|| Placement::Space(shared.clone()));
                    }
                }
            }
        }
        if *deciding {
            for (_, repo) in &machine.repos {
                placement.entry(repo.clone()).or_insert(Placement::Nowhere);
            }
            for collection in &machine.collections.state.collections {
                if let Some(group) = live_collection(
                    collections.shared_id(&machine.machine, &collection.collection_id),
                ) {
                    group_space.entry(group).or_insert(None);
                }
            }
        }
    }

    let space_order: Vec<String> = spaces
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    let mut plan = SharedDocsPlan {
        group_aliases: collections
            .of_machine
            .iter()
            .filter(|((_, own), shared)| own != *shared)
            .filter(|(_, shared)| collections.entry(shared).is_some())
            .map(|(key, shared)| (key.clone(), shared.clone()))
            .collect(),
        ..SharedDocsPlan::default()
    };
    for machine in machines {
        let repo_of: BTreeMap<&str, &str> = machine
            .repos
            .iter()
            .map(|(project, repo)| (project.as_str(), repo.as_str()))
            .collect();
        let collections_doc = machine_collections(machine, &collections, &placement, &repo_of);
        let spaces_doc = machine.spaces.as_ref().map(|document| {
            machine_spaces(
                machine,
                document,
                &spaces,
                &space_order,
                &collections,
                &collections_doc,
                &group_space,
                &placement,
                &repo_of,
            )
        });
        let collections_write =
            (collections_doc.state != machine.collections.state).then(|| collections_doc.clone());
        let spaces_write = spaces_doc.clone().filter(|document| {
            machine
                .spaces
                .as_ref()
                .is_some_and(|current| current.state != document.state)
        });
        if collections_write.is_some() || spaces_write.is_some() {
            plan.writes.push(SharedDocsWrite {
                machine: machine.machine.clone(),
                spaces: spaces_write,
                collections: collections_write,
            });
        }
        plan.snapshots.insert(
            machine.machine.clone(),
            SharedDocsSnapshot {
                spaces: spaces_doc.map(|document| document.state),
                collections: collections_doc.state,
            },
        );
    }
    if first_pass {
        let names: Vec<BTreeSet<String>> = machines
            .iter()
            .filter_map(|machine| machine.spaces.as_ref())
            .map(|document| {
                document
                    .state
                    .spaces
                    .values()
                    .map(|space| normalized(&space.name))
                    .collect()
            })
            .collect();
        plan.names_differ = names.len() > 1 && names.windows(2).any(|pair| pair[0] != pair[1]);
    }
    plan
}

/// One computer's groups: every shared group it has a copy of a project in, those projects in the
/// order it had them, and the members this pass cannot place where it had them.
fn machine_collections(
    machine: &SharedDocsMachine,
    collections: &Identities,
    placement: &BTreeMap<String, Placement>,
    repo_of: &BTreeMap<&str, &str>,
) -> CollectionsDocument {
    let mut out: Vec<Collection> = Vec::new();
    for entry in &collections.entries {
        let own = machine
            .collections
            .state
            .collections
            .iter()
            .find(|collection| {
                collections.shared_id(&machine.machine, &collection.collection_id)
                    == Some(&entry.id)
            });
        let mut project_ids: Vec<String> = Vec::new();
        // Its current members first, in their order: the placed ones still filed here, and the
        // ones this pass cannot place.
        for project_id in own
            .map(|own| own.project_ids.as_slice())
            .unwrap_or_default()
        {
            let keep = match repo_of.get(project_id.as_str()) {
                Some(repo) => {
                    placement.get(*repo) == Some(&Placement::Collection(entry.id.clone()))
                }
                None => true,
            };
            if keep && !project_ids.contains(project_id) {
                project_ids.push(project_id.clone());
            }
        }
        for (project_id, repo) in &machine.repos {
            if placement.get(repo) == Some(&Placement::Collection(entry.id.clone()))
                && !project_ids.contains(project_id)
            {
                project_ids.push(project_id.clone());
            }
        }
        if project_ids.is_empty() {
            continue;
        }
        out.push(Collection {
            collection_id: entry.id.clone(),
            title: entry.name.clone(),
            color: entry.color.clone(),
            project_ids,
        });
    }
    normalized_collections(CollectionsDocument {
        state: CollectionsState { collections: out },
        next_collection_number: machine.collections.next_collection_number,
    })
}

/// One computer's Spaces: every shared Space, holding its own groups and copies.
#[allow(clippy::too_many_arguments)]
fn machine_spaces(
    machine: &SharedDocsMachine,
    current: &SpacesDocument,
    spaces: &Identities,
    order: &[String],
    collections: &Identities,
    collections_doc: &CollectionsDocument,
    group_space: &BTreeMap<String, Option<String>>,
    placement: &BTreeMap<String, Placement>,
    repo_of: &BTreeMap<&str, &str>,
) -> SpacesDocument {
    let held_groups: BTreeSet<&str> = collections_doc
        .state
        .collections
        .iter()
        .map(|collection| collection.collection_id.as_str())
        .collect();
    let in_a_group: BTreeSet<&str> = collections_doc
        .state
        .collections
        .iter()
        .flat_map(|collection| collection.project_ids.iter().map(String::as_str))
        .collect();
    let mut state = SpacesState::default();
    for space_id in order {
        let Some(entry) = spaces.entry(space_id) else {
            continue;
        };
        let own: Option<&Space> =
            current.state.ordered().into_iter().find(|space| {
                spaces.shared_id(&machine.machine, &space.space_id) == Some(space_id)
            });
        let mut member_collection_ids: Vec<String> = Vec::new();
        for collection_id in own
            .map(|own| own.member_collection_ids.as_slice())
            .unwrap_or_default()
        {
            let shared = collections.shared_id(&machine.machine, collection_id);
            let keep = match shared {
                Some(shared) => group_space.get(shared) == Some(&Some(space_id.clone())),
                None => true,
            };
            let id = shared.cloned().unwrap_or_else(|| collection_id.clone());
            if keep && held_groups.contains(id.as_str()) && !member_collection_ids.contains(&id) {
                member_collection_ids.push(id);
            }
        }
        for (group, space) in group_space {
            if space.as_ref() == Some(space_id)
                && held_groups.contains(group.as_str())
                && !member_collection_ids.contains(group)
            {
                member_collection_ids.push(group.clone());
            }
        }
        let mut member_project_ids: Vec<String> = Vec::new();
        for project_id in own
            .map(|own| own.member_project_ids.as_slice())
            .unwrap_or_default()
        {
            let keep = match repo_of.get(project_id.as_str()) {
                Some(repo) => placement.get(*repo) == Some(&Placement::Space(space_id.clone())),
                None => true,
            };
            if keep
                && !in_a_group.contains(project_id.as_str())
                && !member_project_ids.contains(project_id)
            {
                member_project_ids.push(project_id.clone());
            }
        }
        for (project_id, repo) in &machine.repos {
            if placement.get(repo) == Some(&Placement::Space(space_id.clone()))
                && !in_a_group.contains(project_id.as_str())
                && !member_project_ids.contains(project_id)
            {
                member_project_ids.push(project_id.clone());
            }
        }
        state.order.push(space_id.clone());
        state.spaces.insert(
            space_id.clone(),
            Space {
                space_id: space_id.clone(),
                name: entry.name.clone(),
                icon: entry.icon.clone(),
                color: entry.color.clone(),
                member_collection_ids,
                member_project_ids,
                // A workspace id is the computer's own; a Space new to it goes in its default one.
                workspace_id: own.and_then(|own| own.workspace_id.clone()),
            },
        );
    }
    normalized_spaces(SpacesDocument { state })
}

/// The document as the daemon will echo it, so a write is only made when something really moves.
fn normalized_spaces(document: SpacesDocument) -> SpacesDocument {
    SpacesDocument::from_echo_json(&document.to_wire_json()).unwrap_or(document)
}

fn normalized_collections(document: CollectionsDocument) -> CollectionsDocument {
    let next = document.next_collection_number;
    CollectionsDocument::from_echo_json(&document.to_wire_json())
        .map(|mut parsed| {
            parsed.next_collection_number = parsed.next_collection_number.max(next);
            parsed
        })
        .unwrap_or(document)
}

/// Every loaded computer's projects that share a Space with their copies, keyed by repository with
/// the one list's rule (`unified_sidebar::repo`), in each computer's project order.
pub fn shared_repo_keys(
    store: &PresentationStore,
    machines: &[MachineId],
) -> BTreeMap<MachineId, Vec<(String, String)>> {
    let mut identities: Vec<(MachineId, String, RepoIdentity)> = Vec::new();
    for machine in machines {
        let Some(entry) = store.machine(machine) else {
            continue;
        };
        let Some(loaded) = entry.loaded() else {
            continue;
        };
        let meta = crate::sidebar_view::projects::build_project_meta(
            entry,
            entry
                .side_state()
                .workspace_groups
                .as_ref()
                .map(|groups| groups.project_order.as_slice())
                .unwrap_or_default(),
            &BTreeSet::new(),
        );
        for project in loaded.projects() {
            let overlay = meta.overlay(&project.project_id);
            if overlay
                .is_some_and(|overlay| overlay.worktree.is_some() || overlay.bot_profile.is_some())
            {
                continue;
            }
            let origin = project
                .git_remote_origin_url
                .value()
                .filter(|url| !url.is_empty())
                .map(String::as_str);
            identities.push((
                machine.clone(),
                project.project_id.clone(),
                repo_identity(
                    origin,
                    project.path.as_deref().unwrap_or_default(),
                    None,
                    &project.title,
                ),
            ));
        }
    }
    let canonical = merge_repo_identities(identities.iter().map(|(_, _, identity)| identity));
    let mut out: BTreeMap<MachineId, Vec<(String, String)>> = BTreeMap::new();
    for (machine, project_id, identity) in identities {
        let key = canonical
            .get(&identity.key)
            .map_or(identity.key, |merged| merged.key.clone());
        out.entry(machine).or_default().push((project_id, key));
    }
    out
}

/// What the passes keep between app runs (`shared-spaces-last-*.json`): each computer's documents
/// as the last pass left them, and the group renames still owed.
pub fn shared_docs_state_to_json(
    snapshots: &BTreeMap<MachineId, SharedDocsSnapshot>,
    group_aliases: &BTreeMap<(MachineId, String), String>,
) -> serde_json::Value {
    let machine_key = |machine: &MachineId| {
        machine
            .remote_id()
            .unwrap_or(crate::sidebar_view::LOCAL_MACHINE_ID)
            .to_string()
    };
    let mut machines = serde_json::Map::new();
    for (machine, snapshot) in snapshots {
        machines.insert(
            machine_key(machine),
            serde_json::json!({
                "spaces": snapshot.spaces.clone().map(|state| SpacesDocument { state }.to_wire_json()),
                "collections": CollectionsDocument {
                    state: snapshot.collections.clone(),
                    next_collection_number: 1,
                }
                .to_wire_json(),
            }),
        );
    }
    let aliases: Vec<serde_json::Value> = group_aliases
        .iter()
        .map(|((machine, own), shared)| serde_json::json!([machine_key(machine), own, shared]))
        .collect();
    serde_json::json!({ "version": 1, "machines": machines, "groupAliases": aliases })
}

/// [`shared_docs_state_to_json`] read back; anything it cannot read is left out.
#[allow(clippy::type_complexity)]
pub fn shared_docs_state_from_json(
    value: &serde_json::Value,
) -> (
    BTreeMap<MachineId, SharedDocsSnapshot>,
    BTreeMap<(MachineId, String), String>,
) {
    let machine_of = |key: &str| match key {
        crate::sidebar_view::LOCAL_MACHINE_ID => MachineId::Local,
        remote => MachineId::Remote(remote.to_string()),
    };
    let mut snapshots = BTreeMap::new();
    if let Some(machines) = value.get("machines").and_then(serde_json::Value::as_object) {
        for (key, entry) in machines {
            let Some(collections) = entry
                .get("collections")
                .and_then(CollectionsDocument::from_echo_json)
            else {
                continue;
            };
            let spaces = entry
                .get("spaces")
                .filter(|spaces| !spaces.is_null())
                .and_then(SpacesDocument::from_echo_json)
                .map(|document| document.state);
            snapshots.insert(
                machine_of(key),
                SharedDocsSnapshot {
                    spaces,
                    collections: collections.state,
                },
            );
        }
    }
    let mut aliases = BTreeMap::new();
    for alias in value
        .get("groupAliases")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let text = |index: usize| alias.get(index).and_then(serde_json::Value::as_str);
        if let (Some(machine), Some(own), Some(shared)) = (text(0), text(1), text(2)) {
            aliases.insert((machine_of(machine), own.to_string()), shared.to_string());
        }
    }
    (snapshots, aliases)
}
