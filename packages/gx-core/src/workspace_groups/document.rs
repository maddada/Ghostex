//! The workspace session groups document: the user's own named groups, and the project order.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! This is a document the CLIENT owns, not a view of daemon state. gxserver keeps a durable copy so
//! the phone draws the same groups, but the copy is a write-through and not the source
//! (`CDXC:Sessions 2026-07-12-00:00`: "localStorage stays the instant-edit source"). So the shape
//! here is the stored one, parsed exactly as `parseGpuiWorkspaceSessionGroupsState` parses it,
//! including the parts that look like defensive noise and are not: a group whose title is blank or
//! all spaces falls back to its OWN ID, a project whose entry normalizes to nothing at all is
//! dropped rather than kept empty, and `nextGroupNumber` is derived from the `group-<n>` ids when
//! the stored one is missing or below 2, which is what stops a new group reusing a name the user
//! already has on screen.
//!
//! SEE-ALSO: tooling/gx-core/workspace-session-groups-frozen.ts,
//! packages/gx-core/src/workspace_groups/sync.rs.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

/// One named group inside a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSubgroup {
    pub group_id: String,
    pub session_ids: Vec<String>,
    pub title: String,
}

/// A project's groups, and the number the next one is named after.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectWorkspaceGroups {
    pub groups: Vec<WorkspaceSubgroup>,
    pub next_group_number: i64,
}

/// The whole document.
///
/// `projects` is a sorted map rather than an insertion-ordered one on purpose: the order of that
/// object is not data, `projectOrder` is, and a sorted map makes two equal documents compare equal
/// whatever order they arrived in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceGroupsDocument {
    pub project_order: Vec<String>,
    pub projects: BTreeMap<String, ProjectWorkspaceGroups>,
}

impl WorkspaceGroupsDocument {
    /// `isEmptyGpuiWorkspaceSessionGroupsState`, which decides whether the server's copy is worth
    /// adopting and whether the stored key is written or removed.
    pub fn is_empty(&self) -> bool {
        self.project_order.is_empty() && self.projects.is_empty()
    }

    /// `parseGpuiWorkspaceSessionGroupsState`. Anything that is not an object at all is an empty
    /// document, which is what makes a corrupt key harmless.
    pub fn parse(value: &Value) -> Self {
        let Some(record) = value.as_object() else {
            return Self::default();
        };
        let project_order = record
            .get("projectOrder")
            .and_then(Value::as_array)
            .map(|order| {
                order
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut projects = BTreeMap::new();
        if let Some(stored) = record.get("projects").and_then(Value::as_object) {
            for (project_id, value) in stored {
                if let Some(normalized) = ProjectWorkspaceGroups::parse(value) {
                    projects.insert(project_id.clone(), normalized);
                }
            }
        }
        Self {
            project_order,
            projects,
        }
    }

    /// The document in the shape the stored key and the server copy both hold.
    pub fn to_json(&self) -> Value {
        let mut projects = Map::new();
        for (project_id, groups) in &self.projects {
            projects.insert(project_id.clone(), groups.to_json());
        }
        json!({
            "projectOrder": self.project_order,
            "projects": Value::Object(projects),
        })
    }
}

impl ProjectWorkspaceGroups {
    /// `normalizeStoredProjectGroups`, which answers `None` for an entry that carries nothing.
    fn parse(value: &Value) -> Option<Self> {
        let record = value.as_object()?;
        let mut groups = Vec::new();
        if let Some(stored) = record.get("groups").and_then(Value::as_array) {
            for entry in stored {
                let Some(group) = entry.as_object() else {
                    continue;
                };
                let Some(group_id) = group.get("groupId").and_then(Value::as_str) else {
                    continue;
                };
                if group_id.is_empty() {
                    continue;
                }
                let session_ids = group
                    .get("sessionIds")
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_str)
                            .filter(|id| !id.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                // A title of spaces is not a title: the test is on the TRIMMED value and the
                // fallback is the group's own id, never the trimmed string, so a title the user
                // padded keeps its padding.
                let title = group
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| !crate::sidebar_view::text::js_trim(title).is_empty())
                    .unwrap_or(group_id);
                groups.push(WorkspaceSubgroup {
                    group_id: group_id.to_string(),
                    session_ids,
                    title: title.to_string(),
                });
            }
        }
        let next_group_number = record
            .get("nextGroupNumber")
            .and_then(Value::as_i64)
            .filter(|number| *number >= 2)
            .unwrap_or_else(|| {
                // `Math.max(2, ...groups.map(...))`: the number after the highest `group-<n>` the
                // project already has, so a new group cannot be named after one on screen.
                groups
                    .iter()
                    .map(|group| match group_id_number(&group.group_id) {
                        Some(number) => number.saturating_add(1),
                        None => 2,
                    })
                    .chain(std::iter::once(2))
                    .max()
                    .unwrap_or(2)
            });
        // An entry with no groups and nothing to remember is dropped, not stored empty.
        if groups.is_empty() && next_group_number == 2 {
            return None;
        }
        Some(Self {
            groups,
            next_group_number,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "groups": self
                .groups
                .iter()
                .map(|group| json!({
                    "groupId": group.group_id,
                    "sessionIds": group.session_ids,
                    "title": group.title,
                }))
                .collect::<Vec<_>>(),
            "nextGroupNumber": self.next_group_number,
        })
    }
}

/// `/^group-(\d+)$/`, which is the only id shape the numbering reads.
fn group_id_number(group_id: &str) -> Option<i64> {
    let rest = group_id.strip_prefix("group-")?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

// The `gpui-wsg:` id helpers live in `keys` and are NOT repeated here.
// `encode_workspace_subgroup_id` and `parse_workspace_subgroup_id` already answer both halves,
// against a `ProjectKey` rather than a raw string, which is what every other id in this crate is
// keyed by. The first cut wrote a second pair over raw strings and the compiler caught the clash;
// two parsers for one id shape is how the machine-scoped and raw forms drifted apart in M4d.
