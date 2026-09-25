//! The edits the moves make to the workspace session groups document.
//!
//! CDXC:Sessions 2026-09-21 WHY:
//! Every function here returns `Option<WorkspaceGroupsDocument>`, and `None` does NOT mean "nothing
//! changed". It means the TypeScript returned the SAME OBJECT, which is the only thing its callers
//! test (`if (next === this.workspaceGroups) return`). `withProjectGroups` builds a new object on
//! every other path, so a reorder that puts a row back where it already was still writes the key
//! and still schedules a push, and a port that answered "no change" for it would make one fewer
//! storage write and one fewer push than the app makes today. The difference is only visible in a
//! step-by-step comparison, which is what the parity gate did until the TypeScript was deleted, so the identity semantics are modelled
//! rather than improved.
//!
//! Ported from the TypeScript `createGpuiWorkspaceSessionSubgroup`, `moveGpuiWorkspaceSessionToSubgroup`
//! and `syncGpuiWorkspaceSessionOrderInSubgroup` (their frozen copy,
//! `tooling/gx-core/workspace-session-groups-frozen.ts`, is deleted; see git history).
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/order_write.rs.

use std::collections::BTreeSet;

use ghostex_gx_protocol::WorkspaceSessionGroupsState;

use super::document::{ProjectWorkspaceGroups, WorkspaceGroupsDocument, WorkspaceSubgroup};

/// `GPUI_WORKSPACE_SESSION_GROUP_MAX_COUNT`. The implicit project group counts towards it, which is
/// why the test is `groups.length + 1 >= MAX`.
pub const WORKSPACE_SESSION_GROUP_MAX_COUNT: usize = 20;

impl WorkspaceGroupsDocument {
    /// The document the store holds for a machine, normalized exactly as the stored key is.
    ///
    /// The conversion goes through `to_json`/`parse` rather than field by field on purpose: a
    /// second normalizer is how the machine-scoped and raw id forms drifted apart in M4d, and the
    /// blank-title fallback and the `nextGroupNumber` derivation only exist in `parse`.
    pub fn from_side_state(state: &WorkspaceSessionGroupsState) -> Self {
        match serde_json::to_value(state) {
            Ok(value) => Self::parse(&value),
            Err(_) => Self::default(),
        }
    }

    /// The document in the shape the store's side state holds, so an edit redraws the list.
    pub fn to_side_state(&self) -> WorkspaceSessionGroupsState {
        serde_json::from_value(self.to_json()).unwrap_or_default()
    }

    /// `projectGroups`: a project with no entry reads as an empty one, never as absent.
    fn project_groups(&self, project_id: &str) -> ProjectWorkspaceGroups {
        self.projects
            .get(project_id)
            .cloned()
            .unwrap_or(ProjectWorkspaceGroups {
                groups: Vec::new(),
                next_group_number: 2,
            })
    }

    /// `withProjectGroups`: an entry that holds nothing is deleted rather than stored empty, which
    /// is what keeps `is_empty` (and therefore the storage REMOVE) reachable.
    fn with_project_groups(&self, project_id: &str, next: ProjectWorkspaceGroups) -> Self {
        let mut document = self.clone();
        if next.groups.is_empty() && next.next_group_number == 2 {
            document.projects.remove(project_id);
        } else {
            document.projects.insert(project_id.to_string(), next);
        }
        document
    }

    /// `createGpuiWorkspaceSessionSubgroup`. `None` is the group limit, which the caller turns into
    /// a toast and no write at all.
    pub fn create_subgroup(
        &self,
        project_id: &str,
        initial_session_id: Option<&str>,
    ) -> Option<(String, Self)> {
        let current = self.project_groups(project_id);
        if current.groups.len() + 1 >= WORKSPACE_SESSION_GROUP_MAX_COUNT {
            return None;
        }
        let group_number = current.next_group_number;
        let group_id = format!("group-{group_number}");
        let mut groups = match initial_session_id {
            Some(session_id) => remove_session_from_subgroups(&current, session_id).groups,
            None => current.groups.clone(),
        };
        groups.push(WorkspaceSubgroup {
            group_id: group_id.clone(),
            session_ids: initial_session_id
                .map(|session_id| vec![session_id.to_string()])
                .unwrap_or_default(),
            title: format!("Group {group_number}"),
        });
        let next = ProjectWorkspaceGroups {
            groups,
            next_group_number: group_number.saturating_add(1),
        };
        Some((group_id, self.with_project_groups(project_id, next)))
    }

    /// `moveGpuiWorkspaceSessionToSubgroup`. `None` is the one identity return it has: a target
    /// group the project does not hold.
    ///
    /// `target_index` is clamped, never trusted: it is computed against the list the SIDEBAR draws
    /// the group from and applied to the list the DOCUMENT holds, and those two are the same list
    /// only because a user-made group draws exactly its own members. The clamp is what the
    /// TypeScript does and what keeps a stale index from panicking here.
    pub fn move_session_to_subgroup(
        &self,
        project_id: &str,
        session_id: &str,
        target_group_id: Option<&str>,
        target_index: Option<i64>,
    ) -> Option<Self> {
        let current = self.project_groups(project_id);
        let without_session = remove_session_from_subgroups(&current, session_id);
        let Some(target_group_id) = target_group_id else {
            return Some(self.with_project_groups(project_id, without_session));
        };
        if !without_session
            .groups
            .iter()
            .any(|group| group.group_id == target_group_id)
        {
            return None;
        }
        let groups = without_session
            .groups
            .iter()
            .map(|group| {
                if group.group_id != target_group_id {
                    return group.clone();
                }
                let mut session_ids = group.session_ids.clone();
                let clamped = match target_index {
                    None => session_ids.len(),
                    Some(index) => index.clamp(0, session_ids.len() as i64) as usize,
                };
                session_ids.insert(clamped, session_id.to_string());
                WorkspaceSubgroup {
                    session_ids,
                    ..group.clone()
                }
            })
            .collect();
        Some(self.with_project_groups(
            project_id,
            ProjectWorkspaceGroups {
                groups,
                ..without_session
            },
        ))
    }

    /// `syncGpuiWorkspaceSessionOrderInSubgroup`: the given order first, restricted to the group's
    /// own members, then whatever the group holds that the order did not name.
    ///
    /// `None` is its identity return: the group is not there.
    pub fn sync_subgroup_session_order(
        &self,
        project_id: &str,
        group_id: &str,
        session_ids: &[String],
    ) -> Option<Self> {
        let current = self.project_groups(project_id);
        let group = current
            .groups
            .iter()
            .find(|candidate| candidate.group_id == group_id)?;
        let ordered: Vec<String> = session_ids
            .iter()
            .filter(|session_id| group.session_ids.contains(session_id))
            .cloned()
            .collect();
        let remaining = group
            .session_ids
            .iter()
            .filter(|session_id| !ordered.contains(session_id))
            .cloned();
        let next_ids: Vec<String> = ordered.iter().cloned().chain(remaining).collect();
        let groups = current
            .groups
            .iter()
            .map(|candidate| {
                if candidate.group_id == group_id {
                    WorkspaceSubgroup {
                        session_ids: next_ids.clone(),
                        ..candidate.clone()
                    }
                } else {
                    candidate.clone()
                }
            })
            .collect();
        Some(self.with_project_groups(project_id, ProjectWorkspaceGroups { groups, ..current }))
    }

    /// `syncGpuiWorkspaceProjectOrder`: the manual project order, de-duplicated.
    ///
    /// `None` is its identity return, and it is the ONLY edit in this family that has one on a
    /// value comparison rather than on a missing entry: an order equal to the one already held
    /// changes nothing, so a project reorder that put every row back where it was writes no key and
    /// books no push. The de-duplication happens BEFORE the comparison, so an order naming a
    /// project twice is equal to the same order naming it once.
    pub fn sync_project_order(&self, ordered_project_ids: &[String]) -> Option<Self> {
        let mut deduped: Vec<String> = Vec::with_capacity(ordered_project_ids.len());
        for project_id in ordered_project_ids {
            if !deduped.contains(project_id) {
                deduped.push(project_id.clone());
            }
        }
        if deduped == self.project_order {
            return None;
        }
        Some(Self {
            project_order: deduped,
            projects: self.projects.clone(),
        })
    }

    /// `syncGpuiWorkspaceSessionSubgroupOrder`: a project's user-made groups in the order the
    /// sidebar now draws them, then whatever the order did not name.
    ///
    /// `None` is its identity return: the project has no groups at all. A project WITH groups
    /// always comes back as a new object, even when the order did not move, which is what makes a
    /// project reorder write the key for a document that did not change.
    pub fn sync_subgroup_order(
        &self,
        project_id: &str,
        ordered_group_ids: &[String],
    ) -> Option<Self> {
        let current = self.project_groups(project_id);
        if current.groups.is_empty() {
            return None;
        }
        // `orderedGroupIds.map(byId.get).filter(defined)`, with NO de-duplication: an order naming
        // one group twice really does put it in twice there. Unreachable from the caller, whose
        // ids come from the sidebar's own group order, and written the same way rather than
        // "fixed", because a guard that cannot fire is a guard nobody can check.
        let mut groups: Vec<WorkspaceSubgroup> = Vec::with_capacity(current.groups.len());
        for group_id in ordered_group_ids {
            if let Some(group) = current
                .groups
                .iter()
                .find(|candidate| candidate.group_id == *group_id)
            {
                groups.push(group.clone());
            }
        }
        for group in &current.groups {
            if !groups.iter().any(|kept| kept.group_id == group.group_id) {
                groups.push(group.clone());
            }
        }
        Some(self.with_project_groups(project_id, ProjectWorkspaceGroups { groups, ..current }))
    }

    /// `pruneGpuiWorkspaceSessionSubgroups`: drop every member of this project's groups whose
    /// session the presentation no longer lists.
    ///
    /// `None` is its identity return and it has TWO shapes, both of which the TypeScript expresses
    /// as "the same object came back": the project has no entry at all, and no group lost a member.
    /// A caller that treated either as an edit would write the key and push a document that did not
    /// move.
    pub fn prune_project_sessions(
        &self,
        project_id: &str,
        existing_session_ids: &BTreeSet<String>,
    ) -> Option<Self> {
        let current = self.projects.get(project_id)?;
        let mut changed = false;
        let groups: Vec<WorkspaceSubgroup> = current
            .groups
            .iter()
            .map(|group| {
                let session_ids: Vec<String> = group
                    .session_ids
                    .iter()
                    .filter(|session_id| existing_session_ids.contains(*session_id))
                    .cloned()
                    .collect();
                if session_ids.len() == group.session_ids.len() {
                    return group.clone();
                }
                changed = true;
                WorkspaceSubgroup {
                    session_ids,
                    ..group.clone()
                }
            })
            .collect();
        if !changed {
            return None;
        }
        Some(self.with_project_groups(
            project_id,
            ProjectWorkspaceGroups {
                groups,
                ..current.clone()
            },
        ))
    }

    /// `pruneWorkspaceGroupAssignments`: the same prune over every project the presentation lists,
    /// threaded through one document so a pass that drops members from three projects is ONE edit.
    ///
    /// A project the caller does not name is left exactly as it is. That is the whole reason this
    /// takes the presentation's projects rather than walking the document: a machine whose rows have
    /// not arrived lists no projects, and pruning its entries against the nothing it has would
    /// delete every group the user made on it.
    pub fn prune_projects<'a>(
        &self,
        projects: impl IntoIterator<Item = (&'a str, &'a BTreeSet<String>)>,
    ) -> Option<Self> {
        let mut next: Option<Self> = None;
        for (project_id, existing_session_ids) in projects {
            let current = next.as_ref().unwrap_or(self);
            if let Some(pruned) = current.prune_project_sessions(project_id, existing_session_ids) {
                next = Some(pruned);
            }
        }
        next
    }

    /// The user-made group a session belongs to, or `None` for one in the project's own list.
    /// `findGpuiWorkspaceSessionSubgroupForSession`.
    pub fn subgroup_of_session(&self, project_id: &str, session_id: &str) -> Option<&str> {
        self.projects
            .get(project_id)?
            .groups
            .iter()
            .find(|group| group.session_ids.iter().any(|held| held == session_id))
            .map(|group| group.group_id.as_str())
    }
}

/// `removeSessionFromSubgroups`. Its own identity shortcut is deliberately not modelled: its one
/// caller pair always passes the result to `withProjectGroups`, which builds a new object either
/// way, so the shortcut can never reach a caller that tests for it.
fn remove_session_from_subgroups(
    current: &ProjectWorkspaceGroups,
    session_id: &str,
) -> ProjectWorkspaceGroups {
    ProjectWorkspaceGroups {
        groups: current
            .groups
            .iter()
            .map(|group| WorkspaceSubgroup {
                session_ids: group
                    .session_ids
                    .iter()
                    .filter(|held| *held != session_id)
                    .cloned()
                    .collect(),
                ..group.clone()
            })
            .collect(),
        next_group_number: current.next_group_number,
    }
}
