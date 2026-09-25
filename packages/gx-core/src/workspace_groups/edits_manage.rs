//! The two edits the project menu and a group's own menu make that no move makes: renaming a
//! user-made group and removing it.
//!
//! The identity rule is the one `edits.rs` explains: `None` is the TypeScript returning the SAME
//! object, which is the only thing its callers test, and every other path builds a new document
//! even when nothing in it moved.
//!
//! SEE-ALSO: tooling/gx-core/workspace-session-groups-frozen.ts (`renameGpuiWorkspaceSessionSubgroup`,
//! `removeGpuiWorkspaceSessionSubgroup`), packages/gx-core/src/workspace_groups/group_commands.rs.

use crate::sidebar_view::text::js_trim;

use super::document::{ProjectWorkspaceGroups, WorkspaceGroupsDocument, WorkspaceSubgroup};

impl WorkspaceGroupsDocument {
    /// `renameGpuiWorkspaceSessionSubgroup`: the title trimmed; `None` for a blank title or a group
    /// the project does not hold.
    pub fn rename_subgroup(&self, project_id: &str, group_id: &str, title: &str) -> Option<Self> {
        let trimmed = js_trim(title);
        if trimmed.is_empty() {
            return None;
        }
        let current = self.projects.get(project_id)?;
        if !current
            .groups
            .iter()
            .any(|group| group.group_id == group_id)
        {
            return None;
        }
        let groups = current
            .groups
            .iter()
            .map(|group| match group.group_id == group_id {
                true => WorkspaceSubgroup {
                    title: trimmed.to_string(),
                    ..group.clone()
                },
                false => group.clone(),
            })
            .collect();
        Some(self.with_groups(
            project_id,
            ProjectWorkspaceGroups {
                groups,
                next_group_number: current.next_group_number,
            },
        ))
    }

    /// `removeGpuiWorkspaceSessionSubgroup`; `None` for a group the project does not hold.
    pub fn remove_subgroup(&self, project_id: &str, group_id: &str) -> Option<Self> {
        let current = self.projects.get(project_id)?;
        if !current
            .groups
            .iter()
            .any(|group| group.group_id == group_id)
        {
            return None;
        }
        let groups = current
            .groups
            .iter()
            .filter(|group| group.group_id != group_id)
            .cloned()
            .collect();
        Some(self.with_groups(
            project_id,
            ProjectWorkspaceGroups {
                groups,
                next_group_number: current.next_group_number,
            },
        ))
    }

    /// The raw session ids a user-made group holds, in its order; empty for a group that is not
    /// there (`getGpuiWorkspaceSessionSubgroups(...).find(...)?.sessionIds ?? []`).
    pub fn subgroup_members(&self, project_id: &str, group_id: &str) -> Vec<String> {
        self.projects
            .get(project_id)
            .and_then(|project| {
                project
                    .groups
                    .iter()
                    .find(|group| group.group_id == group_id)
            })
            .map(|group| group.session_ids.clone())
            .unwrap_or_default()
    }

    /// `withProjectGroups`, which deletes an entry that holds nothing.
    fn with_groups(&self, project_id: &str, next: ProjectWorkspaceGroups) -> Self {
        let mut document = self.clone();
        if next.groups.is_empty() && next.next_group_number == 2 {
            document.projects.remove(project_id);
        } else {
            document.projects.insert(project_id.to_string(), next);
        }
        document
    }
}
