//! Spaces and project groups as synced keys, and back.
//!
//! Keys are `spaces/<kind>/<field>/<id>`:
//! - `space/exists|name|icon|color|pos/<spaceId>`
//! - `group/exists|title|color|space|pos/<groupId>` (`space` is the Space the group is in, or null)
//! - `repo/place|pos/<repository key>`: where a repository is filed, `{"space": id}`,
//!   `{"group": id}` or null, and its order there.
//!
//! A repository is the same one on every computer (`unified_sidebar::repo`), so each computer files
//! its own copy of it where the shared entry says. Spaces and groups keep one id everywhere.
//!
//! CDXC:Spaces 2026-11-06 DECISION:
//! User: "we need to rethink spaces", then Q1354 "merge today's Spaces by name" (with a toast saying to name Spaces the same on each computer so they get grouped), Q1355 "kept on every computer" and Q1356 "each machine's tab uses the shared Spaces narrowed to that machine's projects": the same Spaces and groups (same ids, names, icons, colours, order) on every computer, and a repository in the same Space or group on all of them, each computer listing its own copies. Supersedes the desktop pass of 2026-11-05 (gx-core `project_docs::shared`, removed): settings sync in gxserver now keeps them alike (Q1372, "our own per-field merge"), so it works while no desktop app is open and reaches computers the app never shows together.
//!
//! CDXC:SettingsSync 2026-11-06 WHY:
//! Only a Space leaving this computer's document is a deletion. A group also leaves a computer's document when its last project here moves out (gxserver drops empty groups), and a repository when its checkout is removed here, while the other computers still have theirs; so those absences say nothing, and a group is deleted everywhere only through an explicit Ungroup (`group_deleted_edit`). A project this computer cannot share (a worktree, a bot, a hidden one) stays where it is here.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::entries::{SyncConflict, SyncEntry};

/// This computer's two documents (normalized, as gxserver stores them) and its shareable projects.
#[derive(Clone, Copy, Debug)]
pub struct LocalSpaces<'a> {
    pub spaces: &'a Value,
    pub collections: &'a Value,
    /// Project id → repository key, for the projects that are filed like their copies elsewhere,
    /// in this computer's project order.
    pub repos: &'a [(String, String)],
}

/// The documents the entries call for on this computer.
#[derive(Clone, Debug, PartialEq)]
pub struct SpacesApplied {
    pub spaces: Value,
    pub collections: Value,
}

fn key(kind: &str, field: &str, id: &str) -> String {
    format!("spaces/{kind}/{field}/{id}")
}

fn parse_key(key: &str) -> Option<(&str, &str, &str)> {
    let mut parts = key.splitn(4, '/');
    if parts.next()? != "spaces" {
        return None;
    }
    Some((parts.next()?, parts.next()?, parts.next()?))
}

/// The edits this computer made since `baseline` (what the last sync left it with; `None` before
/// its first sync, when everything it has is new), as (key, value) pairs; `None` deletes.
pub fn derive_spaces_edits(
    current: LocalSpaces<'_>,
    baseline: Option<LocalSpaces<'_>>,
    entries: &BTreeMap<String, SyncEntry>,
) -> Vec<(String, Option<Value>)> {
    let now = values(current);
    let before = baseline.map(values).unwrap_or_default();
    let mut edits: Vec<(String, Option<Value>)> = now
        .iter()
        .filter(|(key, value)| before.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), Some(value.clone())))
        .collect();
    for key in before.keys() {
        if !now.contains_key(key)
            && parse_key(key).is_some_and(|(kind, field, _)| kind == "space" && field == "exists")
        {
            edits.push((key.clone(), None));
        }
    }
    let pos = |kind: &str, id: &str| -> Option<f64> {
        entries
            .get(&key(kind, "pos", id))
            .and_then(|entry| entry.value.as_ref())
            .and_then(Value::as_f64)
    };
    let mut orders: Vec<(&str, Vec<String>)> = vec![
        ("space", ordered_ids(current.spaces, "spaces")),
        ("group", ordered_ids(current.collections, "collections")),
    ];
    let repo_of: BTreeMap<&str, &str> = current
        .repos
        .iter()
        .map(|(project, repo)| (project.as_str(), repo.as_str()))
        .collect();
    let repo_lists = members_by_container(current).into_values().map(|projects| {
        let mut repos: Vec<String> = Vec::new();
        for project in projects {
            if let Some(repo) = repo_of.get(project.as_str()) {
                if !repos.iter().any(|known| known == repo) {
                    repos.push((*repo).to_string());
                }
            }
        }
        repos
    });
    orders.extend(repo_lists.map(|repos| ("repo", repos)));
    for (kind, ids) in orders {
        for (id, position) in order_edits(&ids, |id| pos(kind, id)) {
            edits.push((key(kind, "pos", &id), Some(json!(position))));
        }
    }
    edits
}

/// What this computer's documents say, as synced keys (positions aside).
fn values(local: LocalSpaces<'_>) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut group_space: BTreeMap<String, String> = BTreeMap::new();
    let mut project_space: BTreeMap<String, String> = BTreeMap::new();
    for (space_id, space) in ordered(local.spaces, "spaces") {
        out.insert(key("space", "exists", &space_id), json!(true));
        for field in ["name", "icon", "color"] {
            if let Some(text) = space.get(field).and_then(Value::as_str) {
                out.insert(key("space", field, &space_id), json!(text));
            }
        }
        for group in strings(space.get("memberCollectionIds")) {
            group_space.entry(group).or_insert_with(|| space_id.clone());
        }
        for project in strings(space.get("memberProjectIds")) {
            project_space
                .entry(project)
                .or_insert_with(|| space_id.clone());
        }
    }
    let mut project_group: BTreeMap<String, String> = BTreeMap::new();
    for (group_id, group) in ordered(local.collections, "collections") {
        out.insert(key("group", "exists", &group_id), json!(true));
        for field in ["title", "color"] {
            if let Some(text) = group.get(field).and_then(Value::as_str) {
                out.insert(key("group", field, &group_id), json!(text));
            }
        }
        out.insert(
            key("group", "space", &group_id),
            group_space
                .get(&group_id)
                .map_or(Value::Null, |space| json!(space)),
        );
        for project in strings(group.get("projectIds")) {
            project_group
                .entry(project)
                .or_insert_with(|| group_id.clone());
        }
    }
    for (project, repo) in local.repos {
        let place_key = key("repo", "place", repo);
        if out.contains_key(&place_key) {
            continue;
        }
        let place = match (project_group.get(project), project_space.get(project)) {
            (Some(group), _) => json!({ "group": group }),
            (None, Some(space)) => json!({ "space": space }),
            (None, None) => Value::Null,
        };
        out.insert(place_key, place);
    }
    out
}

/// New positions for the ids of a list whose stored positions don't follow its order (or that
/// have none yet), leaving every id that is already in order alone.
fn order_edits(ids: &[String], pos: impl Fn(&str) -> Option<f64>) -> Vec<(String, f64)> {
    let mut edits = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    for (index, id) in ids.iter().enumerate() {
        if let Some(position) = pos(id).filter(|position| *position > previous) {
            previous = position;
            continue;
        }
        let next = ids[index + 1..]
            .iter()
            .filter_map(|id| pos(id))
            .find(|position| *position > previous);
        let position = match (previous.is_finite(), next) {
            (true, Some(next)) => (previous + next) / 2.0,
            (false, Some(next)) => next - 1.0,
            (true, None) => previous + 1.0,
            (false, None) => 0.0,
        };
        edits.push((id.clone(), position));
        previous = position;
    }
    edits
}

/// Where a project goes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Placement {
    Group(String),
    Space(String),
    Nowhere,
}

/// This computer's documents rebuilt from the entries.
pub fn apply_spaces_entries(
    entries: &BTreeMap<String, SyncEntry>,
    local: LocalSpaces<'_>,
) -> SpacesApplied {
    let value_of = |kind: &str, field: &str, id: &str| {
        entries
            .get(&key(kind, field, id))
            .and_then(|entry| entry.value.as_ref())
    };
    let live = |kind: &str, id: &str| value_of(kind, "exists", id) == Some(&Value::Bool(true));
    let sorted_live = |kind: &str| {
        let mut ids: Vec<(f64, String)> = entries
            .keys()
            .filter_map(|key| parse_key(key))
            .filter(|(k, field, id)| *k == kind && *field == "exists" && live(kind, id))
            .map(|(_, _, id)| {
                (
                    value_of(kind, "pos", id)
                        .and_then(Value::as_f64)
                        .unwrap_or(f64::MAX),
                    id.to_string(),
                )
            })
            .collect();
        ids.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        ids.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    };
    let spaces = sorted_live("space");
    let groups = sorted_live("group");
    let group_space = |group: &str| -> Option<String> {
        value_of("group", "space", group)
            .and_then(Value::as_str)
            .filter(|space| live("space", space))
            .map(str::to_string)
    };
    let resolve = |placement: Placement| match placement {
        Placement::Group(group) if live("group", &group) => Placement::Group(group),
        Placement::Group(group) => group_space(&group).map_or(Placement::Nowhere, Placement::Space),
        Placement::Space(space) if live("space", &space) => Placement::Space(space),
        Placement::Space(_) | Placement::Nowhere => Placement::Nowhere,
    };

    // Where each project is here now, in this computer's order.
    let mut current: Vec<(String, Placement)> = Vec::new();
    for (group_id, group) in ordered(local.collections, "collections") {
        for project in strings(group.get("projectIds")) {
            if !current.iter().any(|(known, _)| *known == project) {
                current.push((project, Placement::Group(group_id.clone())));
            }
        }
    }
    for (space_id, space) in ordered(local.spaces, "spaces") {
        for project in strings(space.get("memberProjectIds")) {
            if !current.iter().any(|(known, _)| *known == project) {
                current.push((project, Placement::Space(space_id.clone())));
            }
        }
    }
    let repo_of: BTreeMap<&str, &str> = local
        .repos
        .iter()
        .map(|(project, repo)| (project.as_str(), repo.as_str()))
        .collect();
    let place_of_repo = |repo: &str| -> Option<Placement> {
        let place = value_of("repo", "place", repo)?;
        Some(match place {
            Value::Object(place) => match (
                place.get("group").and_then(Value::as_str),
                place.get("space").and_then(Value::as_str),
            ) {
                (Some(group), _) => Placement::Group(group.to_string()),
                (None, Some(space)) => Placement::Space(space.to_string()),
                (None, None) => Placement::Nowhere,
            },
            _ => Placement::Nowhere,
        })
    };
    let mut placed: Vec<(String, Placement)> = Vec::new();
    for (project, repo) in local.repos {
        let here = current
            .iter()
            .find(|(known, _)| known == project)
            .map(|(_, placement)| placement.clone())
            .unwrap_or(Placement::Nowhere);
        placed.push((
            project.clone(),
            resolve(place_of_repo(repo).unwrap_or(here)),
        ));
    }
    for (project, placement) in &current {
        if !repo_of.contains_key(project.as_str()) {
            placed.push((project.clone(), resolve(placement.clone())));
        }
    }
    let repo_pos = |project: &str| -> f64 {
        repo_of
            .get(project)
            .and_then(|repo| value_of("repo", "pos", repo))
            .and_then(Value::as_f64)
            .unwrap_or(f64::MAX)
    };
    let members_of = |placement: &Placement| -> Vec<String> {
        let mut members: Vec<(usize, &String)> = placed
            .iter()
            .enumerate()
            .filter(|(_, (_, at))| at == placement)
            .map(|(index, (project, _))| (index, project))
            .collect();
        members.sort_by(|a, b| repo_pos(a.1).total_cmp(&repo_pos(b.1)).then(a.0.cmp(&b.0)));
        members
            .into_iter()
            .map(|(_, project)| project.clone())
            .collect()
    };
    let local_text = |document: &Value, list: &str, id: &str, field: &str| -> Option<String> {
        document
            .get(list)
            .and_then(|items| items.get(id))
            .and_then(|item| item.get(field))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let text = |kind: &str, field: &str, id: &str, document: &Value, list: &str, fallback: &str| {
        value_of(kind, field, id)
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| local_text(document, list, id, field))
            .unwrap_or_else(|| fallback.to_string())
    };

    let mut collections = Map::new();
    let mut collection_order = Vec::new();
    for group in &groups {
        let project_ids = members_of(&Placement::Group(group.clone()));
        if project_ids.is_empty() {
            continue;
        }
        collections.insert(
            group.clone(),
            json!({
                "collectionId": group,
                "title": text("group", "title", group, local.collections, "collections", "Group"),
                "color": text("group", "color", group, local.collections, "collections", ""),
                "projectIds": project_ids,
            }),
        );
        collection_order.push(json!(group));
    }
    let next_number = local
        .collections
        .get("nextCollectionNumber")
        .cloned()
        .unwrap_or(json!(1));
    let mut space_map = Map::new();
    let mut space_order = Vec::new();
    for space in &spaces {
        let member_collection_ids: Vec<&String> = groups
            .iter()
            .filter(|group| collections.contains_key(*group))
            .filter(|group| group_space(group).as_deref() == Some(space.as_str()))
            .collect();
        let mut entry = json!({
            "spaceId": space,
            "name": text("space", "name", space, local.spaces, "spaces", "Space"),
            "icon": text("space", "icon", space, local.spaces, "spaces", ""),
            "color": text("space", "color", space, local.spaces, "spaces", ""),
            "memberCollectionIds": member_collection_ids,
            "memberProjectIds": members_of(&Placement::Space(space.clone())),
        });
        if let Some(workspace) = local_text(local.spaces, "spaces", space, "workspaceId") {
            entry["workspaceId"] = json!(workspace);
        }
        space_map.insert(space.clone(), entry);
        space_order.push(json!(space));
    }
    SpacesApplied {
        spaces: json!({ "order": space_order, "spaces": space_map }),
        collections: json!({
            "collections": collections,
            "order": collection_order,
            "nextCollectionNumber": next_number,
        }),
    }
}

/// On the first sync between two computers, Spaces (and groups) with the same name become one:
/// the one with the smaller id stays and the others' contents move into it.
///
/// CDXC:SettingsSync 2026-11-06 WHY:
/// The user chose to merge Spaces by name the first time two computers' Spaces meet (Q1354, CDXC:Spaces 2026-11-06 above). Every computer picks the same survivor from the same entries, so the edits two computers make for one pair agree instead of conflicting. After the first sync only ids match, so a later rename into an existing name doesn't swallow a Space.
pub fn merge_spaces_by_name(entries: &BTreeMap<String, SyncEntry>) -> Vec<(String, Option<Value>)> {
    let mut edits = Vec::new();
    for (kind, name_field, place_field) in [("space", "name", "space"), ("group", "title", "group")]
    {
        let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for key_text in entries.keys() {
            let Some((k, field, id)) = parse_key(key_text) else {
                continue;
            };
            if k != kind || field != "exists" {
                continue;
            }
            let value = |field: &str| {
                entries
                    .get(&key(kind, field, id))
                    .and_then(|e| e.value.clone())
            };
            if value("exists") != Some(Value::Bool(true)) {
                continue;
            }
            let Some(name) = value(name_field).and_then(|name| name.as_str().map(str::to_string))
            else {
                continue;
            };
            let name = name.trim().to_lowercase();
            if !name.is_empty() {
                by_name.entry(name).or_default().insert(id.to_string());
            }
        }
        for ids in by_name.values().filter(|ids| ids.len() > 1) {
            let mut ids = ids.iter();
            let Some(survivor) = ids.next() else {
                continue;
            };
            for duplicate in ids {
                edits.push((key(kind, "exists", duplicate), None));
                for (key_text, entry) in entries {
                    let Some((k, field, _)) = parse_key(key_text) else {
                        continue;
                    };
                    let points_here = entry.value.as_ref().and_then(|value| {
                        if k == "repo" && field == "place" {
                            value.get(place_field).and_then(Value::as_str)
                        } else if kind == "space" && k == "group" && field == "space" {
                            value.as_str()
                        } else {
                            None
                        }
                    }) == Some(duplicate.as_str());
                    if points_here {
                        let moved = if k == "repo" {
                            json!({ place_field: survivor })
                        } else {
                            json!(survivor)
                        };
                        edits.push((key_text.clone(), Some(moved)));
                    }
                }
            }
        }
    }
    edits
}

/// Two computers meeting for the first time hold Spaces whose names don't all match, so the toast
/// asks to name Spaces alike (Q1354): only Spaces with the same name became one.
pub fn spaces_named_apart(here: &BTreeMap<String, SyncEntry>, incoming: &[SyncEntry]) -> bool {
    fn names<'a>(entries: impl Iterator<Item = &'a SyncEntry> + Clone) -> BTreeSet<String> {
        let value = |key: &str| {
            entries
                .clone()
                .find(|entry| entry.key == key)
                .and_then(|entry| entry.value.clone())
        };
        entries
            .clone()
            .filter_map(|entry| {
                let (kind, field, id) = parse_key(&entry.key)?;
                (kind == "space" && field == "exists" && entry.value == Some(Value::Bool(true)))
                    .then(|| value(&key("space", "name", id)))
                    .flatten()
                    .and_then(|name| name.as_str().map(|name| name.trim().to_lowercase()))
            })
            .collect()
    }
    let mine = names(here.values());
    let theirs = names(incoming.iter());
    !mine.is_empty() && !theirs.is_empty() && mine != theirs
}

/// One plain sentence about a conflict on a Spaces key for the "changed on X · Undo" note, or
/// `None` for one not worth a note (an order change).
pub fn describe_spaces_key(
    conflict: &SyncConflict,
    entries: &BTreeMap<String, SyncEntry>,
) -> Option<String> {
    let (kind, field, id) = parse_key(&conflict.kept.key)?;
    if field == "pos" {
        return None;
    }
    let name_of = |kind: &str, id: &str| -> String {
        let field = if kind == "group" { "title" } else { "name" };
        entries
            .get(&key(kind, field, id))
            .and_then(|entry| entry.value.as_ref())
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string()
    };
    let show = |value: &Option<Value>| -> String {
        match value {
            None => "deleted".to_string(),
            Some(Value::Bool(true)) => "kept".to_string(),
            Some(Value::String(text)) if field == "space" => {
                format!("in Space “{}”", name_of("space", text))
            }
            Some(Value::String(text)) => format!("“{text}”"),
            Some(Value::Null) => "not in a Space".to_string(),
            Some(Value::Object(place)) => match (
                place.get("group").and_then(Value::as_str),
                place.get("space").and_then(Value::as_str),
            ) {
                (Some(group), _) => format!("in group “{}”", name_of("group", group)),
                (None, Some(space)) => format!("in Space “{}”", name_of("space", space)),
                _ => "not in a Space".to_string(),
            },
            Some(other) => other.to_string(),
        }
    };
    let subject = match (kind, field) {
        ("space", "name") => "A Space's name".to_string(),
        ("space", "exists") => format!("Space “{}”", name_of("space", id)),
        ("space", other) => format!("The {} of Space “{}”", noun(other), name_of("space", id)),
        ("group", "title") => "A group's name".to_string(),
        ("group", "exists") | ("group", "space") => format!("Group “{}”", name_of("group", id)),
        ("group", other) => format!("The {} of group “{}”", noun(other), name_of("group", id)),
        ("repo", _) => repo_label(id),
        _ => return None,
    };
    Some(format!(
        "{subject} is {} from {}, replacing {} from {}.",
        show(&conflict.kept.value),
        conflict.kept.origin_name,
        show(&conflict.lost.value),
        conflict.lost.origin_name,
    ))
}

fn noun(field: &str) -> &str {
    match field {
        "color" => "colour",
        other => other,
    }
}

fn repo_label(repo: &str) -> String {
    repo.trim_start_matches("dir:")
        .rsplit('/')
        .next()
        .unwrap_or(repo)
        .to_string()
}

/// The edit that deletes a group on every computer (the user's Ungroup).
pub fn group_deleted_edit(group_id: &str) -> (String, Option<Value>) {
    (key("group", "exists", group_id), None)
}

/// A document's items in its `order`, then any it holds outside the order.
fn ordered<'a>(document: &'a Value, list: &str) -> Vec<(String, &'a Value)> {
    let Some(items) = document.get(list).and_then(Value::as_object) else {
        return Vec::new();
    };
    ordered_ids(document, list)
        .into_iter()
        .filter_map(|id| items.get(&id).map(|item| (id, item)))
        .collect()
}

fn ordered_ids(document: &Value, list: &str) -> Vec<String> {
    let Some(items) = document.get(list).and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = strings(document.get("order"))
        .into_iter()
        .filter(|id| items.contains_key(id))
        .collect();
    for id in items.keys() {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// Each group's and each Space's own projects, in their order.
fn members_by_container(local: LocalSpaces<'_>) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for (group_id, group) in ordered(local.collections, "collections") {
        out.insert(
            format!("group:{group_id}"),
            strings(group.get("projectIds")),
        );
    }
    for (space_id, space) in ordered(local.spaces, "spaces") {
        out.insert(
            format!("space:{space_id}"),
            strings(space.get("memberProjectIds")),
        );
    }
    out
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}
