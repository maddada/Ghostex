//! The Projects tab: the sorting, day grouping, search, rows and context menu of
//! packages/core-ui/recent-projects-modal.tsx.
//!
//! Ported from `apps/desktop/sidebar/native-quick-access/projects.ts` (deleted with QuickJS on
//! 2026-09-25; see git history).

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::data::{QuickAccessCollection, QuickAccessHiddenItems};
use super::icons::recent_project_icon;
use super::row_actions::{action_item, item, separator};
use super::text::{
    day_label, js_lower, locale_compare, parse_timestamp, split_words, HotkeyPlatform,
    QuickAccessClock,
};
use super::wire::{QuickAccessGroup, QuickAccessMenuItem, QuickAccessRow};
use crate::keys::{MachineId, ProjectKey};

/// The Projects tab's data: the answer for one machine (none for this computer).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecentProjectsData {
    pub(crate) machine_id: Option<String>,
    pub(crate) projects: Vec<Value>,
}

fn text<'a>(project: &'a Value, key: &str) -> &'a str {
    project[key].as_str().unwrap_or("")
}

fn is_open(project: &Value) -> bool {
    project["isOpen"] == Value::Bool(true)
}

/// `parseClosedAt`.
fn parse_closed_at(value: &Value) -> i64 {
    parse_timestamp(value.as_str()).unwrap_or(0)
}

/// `sortRecentProjects`: open first, then newest, then by title.
fn sort_projects(projects: &mut [&Value]) {
    projects.sort_by(|left, right| {
        let open = is_open(right).cmp(&is_open(left));
        if open != std::cmp::Ordering::Equal {
            return open;
        }
        let stamp = |project: &Value| {
            parse_closed_at(if is_open(project) {
                &project["updatedAt"]
            } else {
                &project["recentClosedAt"]
            })
        };
        stamp(right)
            .cmp(&stamp(left))
            .then_with(|| locale_compare(text(left, "title"), text(right, "title")))
    });
}

/// `filterRecentProjects`: every term of the query in the title or the path.
fn filter_projects<'a>(projects: &'a [Value], query: &str) -> Vec<&'a Value> {
    let normalized = js_lower(crate::sidebar_view::text::js_trim(query));
    if normalized.is_empty() {
        return projects.iter().collect();
    }
    let terms = split_words(&normalized);
    projects
        .iter()
        .filter(|project| {
            let searchable = js_lower(&format!(
                "{}\n{}",
                text(project, "title"),
                text(project, "path")
            ));
            terms.iter().all(|term| searchable.contains(term))
        })
        .collect()
}

/// `sidebarGroupIdForProject`.
fn sidebar_group_id(project_id: &str) -> String {
    match ProjectKey::parse_workspace_project_id(project_id) {
        Some(key) if !matches!(key.machine, MachineId::Local) => key.to_sidebar_group_id(),
        _ => ProjectKey::local(project_id).to_sidebar_group_id(),
    }
}

/// `hiddenProjectState`: hidden groups, plus every project of a hidden local collection.
fn hidden_state(
    hidden: &QuickAccessHiddenItems,
    collections: &[QuickAccessCollection],
) -> (BTreeSet<String>, BTreeSet<String>) {
    let hidden_collections: BTreeSet<&str> = hidden
        .collection_keys
        .iter()
        .filter_map(|key| key.strip_prefix("local:"))
        .collect();
    let local_projects: BTreeSet<String> = collections
        .iter()
        .filter(|collection| hidden_collections.contains(collection.collection_id.as_str()))
        .flat_map(|collection| collection.project_ids.iter().cloned())
        .collect();
    (hidden.group_ids.iter().cloned().collect(), local_projects)
}

pub(crate) fn project_row_key(project_id: &str) -> String {
    format!("project:{project_id}")
}

/// Open projects lead under an "Open in sidebar" heading, then one group per day a project was
/// closed, then a trailing "Earlier" group.
pub(crate) fn project_groups(
    data: Option<&RecentProjectsData>,
    query: &str,
    hidden: &QuickAccessHiddenItems,
    collections: &[QuickAccessCollection],
    clock: &dyn QuickAccessClock,
) -> Vec<QuickAccessGroup> {
    let Some(data) = data else {
        return Vec::new();
    };
    let (hidden_groups, hidden_projects) = hidden_state(hidden, collections);
    let mut sorted = filter_projects(&data.projects, query);
    sort_projects(&mut sorted);
    let mut groups: Vec<(String, Vec<&Value>)> = Vec::new();
    for project in sorted {
        let closed_at = parse_closed_at(&project["recentClosedAt"]);
        let label = if is_open(project) {
            "Open in sidebar".to_string()
        } else if closed_at == 0 {
            "Earlier".to_string()
        } else {
            day_label(closed_at, clock)
        };
        match groups.iter_mut().find(|(heading, _)| *heading == label) {
            Some((_, projects)) => projects.push(project),
            None => groups.push((label, vec![project])),
        }
    }
    groups
        .into_iter()
        .map(|(heading, projects)| QuickAccessGroup {
            key: heading.clone(),
            heading,
            separated: false,
            rows: projects
                .into_iter()
                .map(|project| {
                    let project_id = text(project, "projectId");
                    QuickAccessRow::Project {
                        key: project_row_key(project_id),
                        title: text(project, "title").to_string(),
                        icon: recent_project_icon(project),
                        tooltip: text(project, "path").to_string(),
                        session_count: project["sessionCount"].as_u64().unwrap_or(0),
                        is_open: is_open(project),
                        is_hidden: hidden_groups.contains(&sidebar_group_id(project_id))
                            || hidden_projects.contains(project_id),
                    }
                })
                .collect(),
        })
        .collect()
}

pub(crate) fn find_project<'a>(
    data: Option<&'a RecentProjectsData>,
    key: &str,
) -> Option<&'a Value> {
    data?
        .projects
        .iter()
        .find(|project| project_row_key(text(project, "projectId")) == key)
}

/// `activateRecentProject`.
pub(crate) fn activate_message(project: &Value) -> Value {
    json!({
        "projectId": project["projectId"],
        "type": if is_open(project) { "focusRecentProject" } else { "restoreRecentProject" },
    })
}

/// `removeRecentProject`.
pub(crate) fn remove_message(project: &Value) -> Value {
    json!({
        "projectId": project["projectId"],
        "type": if is_open(project) { "closeProjectFromProjects" } else { "removeRecentProject" },
    })
}

/// The row's actions, in the React context menu's order. Close project is what the row's hover
/// trash did for an open project.
pub(crate) fn project_menu_items(
    project: &Value,
    machine_id: Option<&str>,
    platform: HotkeyPlatform,
) -> Vec<QuickAccessMenuItem> {
    if is_open(project) {
        return vec![
            item("activate", "Open", "folder-open", platform),
            separator(),
            action_item(
                "remove",
                "Close project",
                "trash",
                None,
                true,
                false,
                platform,
            ),
        ];
    }
    let mut items = vec![
        item("activate", "Restore", "rotate-clockwise", platform),
        item("copyPath", "Copy Path", "copy", platform),
    ];
    if machine_id.is_some() {
        items.push(action_item(
            "openLocationUnavailable",
            "Open File/Folder Location",
            "folder-open",
            None,
            false,
            true,
            platform,
        ));
        items.push(item(
            "openTerminal",
            "Open remote terminal here",
            "terminal-2",
            platform,
        ));
    } else {
        items.push(item(
            "openLocation",
            "Open File/Folder Location",
            "folder-open",
            platform,
        ));
    }
    items.push(separator());
    items.push(action_item(
        "remove",
        "Remove project",
        "trash",
        None,
        true,
        false,
        platform,
    ));
    items
}
