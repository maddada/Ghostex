//! The one-time repair of Spaces that settings sync folded together (#488).
//!
//! CDXC:SettingsSync 2026-11-08 WHY:
//! Issue #488: in 11.0.0 settings sync keyed a project by its repository alone, so every folder project of one monorepo (`monorepo/`, `monorepo/trading`, `monorepo/product-x`) shared one `repo/place` entry. The first sync wrote the place of the first of them and then filed all of them there, emptying the reporter's other Spaces and adding projects that were in no Space. Keys now carry the folder (`unified_sidebar::project_repo_key`), and this repair puts the projects of a folded repository back where they were before sync first wrote this computer's Spaces, from the backup it saved then (`backups/settings-sync-spaces-<time>.json`). It only touches a repository whose folder projects all sit in one place now (what the fold left), and only moves a project back into a Space or group that still exists here (a group gxserver dropped once it was empty comes back from the backup), so a Space the user arranged since stays as it is.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::spaces::{ordered, strings, LocalSpaces, SpacesApplied};
use crate::unified_sidebar::repository_of_project_key;

/// Where a project is filed: in a group, directly in a Space, or nowhere.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    Group(String),
    Space(String),
    Nowhere,
}

/// This computer's documents with the projects of every folded repository put back where
/// `backup` (the documents before sync first wrote them) had them, or `None` when nothing moves.
/// `local.repos` must hold the keys with folders (`project_repo_key`).
pub fn repair_folded_spaces(
    local: LocalSpaces<'_>,
    backup_spaces: &Value,
    backup_collections: &Value,
) -> Option<SpacesApplied> {
    let mut by_repository: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (project, key) in local.repos {
        by_repository
            .entry(repository_of_project_key(key))
            .or_default()
            .push((project.as_str(), key.as_str()));
    }
    let mut moves: Vec<(String, Place)> = Vec::new();
    for members in by_repository.values() {
        let keys: BTreeSet<&str> = members.iter().map(|(_, key)| *key).collect();
        if keys.len() < 2 {
            continue;
        }
        let places: Vec<Place> = members
            .iter()
            .map(|(project, _)| place_of(local.spaces, local.collections, project))
            .collect();
        if places.iter().any(|place| *place != places[0]) {
            continue;
        }
        for (project, _) in members {
            let before = place_of(backup_spaces, backup_collections, project);
            if before != places[0] {
                moves.push(((*project).to_string(), before));
            }
        }
    }
    if moves.is_empty() {
        return None;
    }

    let mut spaces = local.spaces.clone();
    let mut collections = local.collections.clone();
    let mut emptied: BTreeSet<String> = BTreeSet::new();
    for (project, place) in moves {
        if !ready(
            &mut spaces,
            &mut collections,
            &place,
            backup_spaces,
            backup_collections,
        ) {
            continue;
        }
        emptied.extend(take_out(&mut spaces, &mut collections, &project));
        put_in(
            &mut spaces,
            &mut collections,
            &project,
            &place,
            backup_spaces,
            backup_collections,
        );
    }
    for group in emptied {
        let empty = collections
            .get("collections")
            .and_then(|items| items.get(&group))
            .is_some_and(|item| strings(item.get("projectIds")).is_empty());
        if empty {
            drop_group(&mut spaces, &mut collections, &group);
        }
    }
    (spaces != *local.spaces || collections != *local.collections).then_some(SpacesApplied {
        spaces,
        collections,
    })
}

/// One project a rewrite of the Spaces documents moved: where it is now, by name (a Space's name
/// or a group's title), or `None` when it is in no Space any more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpacesMove {
    pub project: String,
    pub to: Option<String>,
}

/// The projects that sit somewhere else in `after` than in `before`, in `after`'s order (then the
/// ones only `before` listed).
pub fn spaces_moves(
    before_spaces: &Value,
    before_collections: &Value,
    after_spaces: &Value,
    after_collections: &Value,
) -> Vec<SpacesMove> {
    let mut projects: Vec<String> = Vec::new();
    for (spaces, collections) in [
        (after_spaces, after_collections),
        (before_spaces, before_collections),
    ] {
        let listed = ordered(collections, "collections")
            .into_iter()
            .flat_map(|(_, group)| strings(group.get("projectIds")))
            .chain(
                ordered(spaces, "spaces")
                    .into_iter()
                    .flat_map(|(_, space)| strings(space.get("memberProjectIds"))),
            );
        for project in listed {
            if !projects.contains(&project) {
                projects.push(project);
            }
        }
    }
    let name = |document: &Value, list: &str, id: &str, field: &str| {
        document
            .get(list)
            .and_then(|items| items.get(id))
            .and_then(|item| item.get(field))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| id.to_string())
    };
    projects
        .into_iter()
        .filter_map(|project| {
            let after = place_of(after_spaces, after_collections, &project);
            if after == place_of(before_spaces, before_collections, &project) {
                return None;
            }
            let to = match after {
                Place::Group(group) => {
                    Some(name(after_collections, "collections", &group, "title"))
                }
                Place::Space(space) => Some(name(after_spaces, "spaces", &space, "name")),
                Place::Nowhere => None,
            };
            Some(SpacesMove { project, to })
        })
        .collect()
}

fn place_of(spaces: &Value, collections: &Value, project: &str) -> Place {
    for (group_id, group) in ordered(collections, "collections") {
        if strings(group.get("projectIds"))
            .iter()
            .any(|id| id == project)
        {
            return Place::Group(group_id);
        }
    }
    for (space_id, space) in ordered(spaces, "spaces") {
        if strings(space.get("memberProjectIds"))
            .iter()
            .any(|id| id == project)
        {
            return Place::Space(space_id);
        }
    }
    Place::Nowhere
}

/// The Space `group` is in, in a Spaces document.
fn space_of_group(spaces: &Value, group: &str) -> Option<String> {
    ordered(spaces, "spaces")
        .into_iter()
        .find(|(_, space)| {
            strings(space.get("memberCollectionIds"))
                .iter()
                .any(|id| id == group)
        })
        .map(|(space_id, _)| space_id)
}

fn has(document: &Value, list: &str, id: &str) -> bool {
    document.get(list).and_then(|items| items.get(id)).is_some()
}

/// Whether `place` can take a project here, bringing back a group the fold emptied (gxserver
/// drops an empty group) when its Space is still here.
fn ready(
    spaces: &mut Value,
    collections: &mut Value,
    place: &Place,
    backup_spaces: &Value,
    backup_collections: &Value,
) -> bool {
    match place {
        Place::Nowhere => true,
        Place::Space(space) => has(spaces, "spaces", space),
        Place::Group(group) if has(collections, "collections", group) => true,
        Place::Group(group) => {
            let Some(mut item) = backup_collections
                .get("collections")
                .and_then(|items| items.get(group))
                .cloned()
            else {
                return false;
            };
            let space = space_of_group(backup_spaces, group);
            if space
                .as_ref()
                .is_some_and(|space| !has(spaces, "spaces", space))
            {
                return false;
            }
            item["projectIds"] = json!([]);
            object(collections, "collections").insert(group.clone(), item);
            array(collections, "order").push(json!(group));
            if let Some(space) = space {
                if let Some(entry) = spaces
                    .get_mut("spaces")
                    .and_then(|items| items.get_mut(&space))
                {
                    array(entry, "memberCollectionIds").push(json!(group));
                }
            }
            true
        }
    }
}

/// Takes `project` out of every group and Space, returning the groups it left.
fn take_out(spaces: &mut Value, collections: &mut Value, project: &str) -> Vec<String> {
    let mut left = Vec::new();
    for (group_id, item) in object(collections, "collections").iter_mut() {
        let list = array(item, "projectIds");
        let before = list.len();
        list.retain(|id| id.as_str() != Some(project));
        if list.len() != before {
            left.push(group_id.clone());
        }
    }
    for item in object(spaces, "spaces").values_mut() {
        array(item, "memberProjectIds").retain(|id| id.as_str() != Some(project));
    }
    left
}

fn put_in(
    spaces: &mut Value,
    collections: &mut Value,
    project: &str,
    place: &Place,
    backup_spaces: &Value,
    backup_collections: &Value,
) {
    let (document, backup, list, id, field) = match place {
        Place::Nowhere => return,
        Place::Group(group) => (
            collections,
            backup_collections,
            "collections",
            group,
            "projectIds",
        ),
        Place::Space(space) => (spaces, backup_spaces, "spaces", space, "memberProjectIds"),
    };
    let order = backup
        .get(list)
        .and_then(|items| items.get(id))
        .map(|item| strings(item.get(field)))
        .unwrap_or_default();
    if let Some(item) = document.get_mut(list).and_then(|items| items.get_mut(id)) {
        insert_like(array(item, field), project, &order);
    }
}

/// Inserts `project` into `list` next to the projects it sat beside in `order`.
fn insert_like(list: &mut Vec<Value>, project: &str, order: &[String]) {
    let index_of = |id: &str| list.iter().position(|known| known.as_str() == Some(id));
    let at = order.iter().position(|id| id == project);
    let after = at.and_then(|at| {
        order[..at]
            .iter()
            .rev()
            .find_map(|id| index_of(id))
            .map(|index| index + 1)
    });
    let before = at.and_then(|at| order[at + 1..].iter().find_map(|id| index_of(id)));
    let index = after.or(before).unwrap_or(list.len());
    list.insert(index, json!(project));
}

fn drop_group(spaces: &mut Value, collections: &mut Value, group: &str) {
    object(collections, "collections").remove(group);
    array(collections, "order").retain(|id| id.as_str() != Some(group));
    for item in object(spaces, "spaces").values_mut() {
        array(item, "memberCollectionIds").retain(|id| id.as_str() != Some(group));
    }
}

/// `document[field]` as an object, made one if it is missing or not one.
fn object<'a>(document: &'a mut Value, field: &str) -> &'a mut Map<String, Value> {
    if !document.get(field).is_some_and(Value::is_object) {
        document[field] = json!({});
    }
    document[field]
        .as_object_mut()
        .expect("just made an object")
}

/// `document[field]` as an array, made one if it is missing or not one.
fn array<'a>(document: &'a mut Value, field: &str) -> &'a mut Vec<Value> {
    if !document.get(field).is_some_and(Value::is_array) {
        document[field] = json!([]);
    }
    document[field].as_array_mut().expect("just made an array")
}
