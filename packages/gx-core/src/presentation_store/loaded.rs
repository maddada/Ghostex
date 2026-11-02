//! The daemon state of one loaded machine, and the ordering rule for its rows.

use std::collections::BTreeMap;

use ghostex_gx_protocol::{
    PresentationCapabilities, PresentationGroup, PresentationProject, PresentationSession,
    PresentationSnapshot, SessionKind, SessionSurface, Tri,
};

use crate::overlay::ProjectSessionMap;

/// The daemon state of a loaded machine. Rows here are exactly what the daemon sent; overlays are
/// applied on read.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedPresentation {
    /// Daemon identity of the snapshot. Empty when the snapshot came from an HTTP read, which
    /// does not carry it.
    pub server_id: String,
    /// The highest revision applied or seen for this machine: the value to quote as
    /// `lastRevision` on a reconnect.
    pub revision: i64,
    /// These rows are the LAST SEEN copy of a machine that has not streamed to this run, read back
    /// from client storage, not anything a daemon said to this client.
    ///
    /// CDXC:RemoteMachines 2026-09-21 WHY:
    /// A last-seen snapshot carries the PREVIOUS run's revision, and the revision rules read that
    /// number as "what this client has already been told". Three of them would then be wrong at
    /// once: a delta could ride on yesterday's rows, `presentationSnapshotCurrent` could CONFIRM
    /// them as live, and the host would quote that revision as `lastRevision` on the subscribe
    /// that asks for them. A daemon that restarted has reset its counter, so its live first frames
    /// are LOWER, which is exactly the case where all three go wrong silently and the user keeps
    /// looking at faded rows from yesterday while the machine is connected. So the copy is held
    /// with this flag set, every revisioned frame is refused while it is, and only a full stream
    /// snapshot may replace it, which `apply_snapshot` already does whatever the revision says.
    pub last_seen: bool,
    pub generated_at: String,
    pub capabilities: Option<PresentationCapabilities>,
    pub auto_settle_after_days: Tri<f64>,
    /// Ordered by `(sort_key, project_id)`.
    pub(super) projects: Vec<PresentationProject>,
    /// Ordered by `(sort_key, group_id)`; each group's `session_ids` is the display order.
    pub(super) groups: Vec<PresentationGroup>,
    pub(super) sessions: ProjectSessionMap<PresentationSession>,
}

impl LoadedPresentation {
    pub fn projects(&self) -> &[PresentationProject] {
        &self.projects
    }

    pub fn groups(&self) -> &[PresentationGroup] {
        &self.groups
    }

    pub fn project(&self, project_id: &str) -> Option<&PresentationProject> {
        self.projects
            .iter()
            .find(|project| project.project_id == project_id)
    }

    /// The daemon row of a session, without overlays.
    pub fn server_session(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> Option<&PresentationSession> {
        self.sessions.get(project_id, session_id)
    }

    /// Every daemon row, ordered by `(project_id, session_id)`.
    pub fn server_sessions(&self) -> impl Iterator<Item = &PresentationSession> {
        self.sessions.iter().map(|(_, _, session)| session)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// The daemon row of a session, mutable. The one local edit that uses it is the manual session
    /// reorder, which writes the same `sidebarOrder` and `sortKey` the daemon will send back.
    pub(crate) fn session_mut(
        &mut self,
        project_id: &str,
        session_id: &str,
    ) -> Option<&mut PresentationSession> {
        self.sessions.get_mut(project_id, session_id)
    }

    pub(crate) fn project_sessions(
        &self,
        project_id: &str,
    ) -> Option<&BTreeMap<String, PresentationSession>> {
        self.sessions.project(project_id)
    }

    pub(super) fn from_snapshot(server_id: String, snapshot: PresentationSnapshot) -> Self {
        let mut sessions = ProjectSessionMap::default();
        for session in snapshot.sessions.rows {
            let project_id = session.project_id.clone();
            let session_id = session.session_id.clone();
            sessions.insert(&project_id, &session_id, session);
        }
        let mut loaded = Self {
            server_id,
            revision: snapshot.revision,
            last_seen: false,
            generated_at: snapshot.generated_at,
            capabilities: snapshot.capabilities,
            auto_settle_after_days: snapshot.auto_settle_after_days,
            projects: snapshot.projects.rows,
            groups: snapshot.groups.rows,
            sessions,
        };
        // The daemon already sends these ordered; sorting keeps the order rule in one place for
        // daemons that do not.
        sort_projects(&mut loaded.projects);
        loaded.repair_groups();
        sort_groups(&mut loaded.groups);
        loaded
    }

    /// Makes the groups agree with the rows after a snapshot, the way the delta path already does.
    ///
    /// CDXC:Workarea 2026-09-19 WHY:
    /// A project's tab list is read from its group's `sessionIds`, and the desktop workspace clears every tab of a project whose list reads empty. A snapshot can arrive with a group missing (its row did not fit and was skipped) or with a session missing from its group's list (a bad element was left out). Every project therefore gets its default group, and a group that does not list one of its own sessions is rebuilt from the rows, in the daemon's order (byte order of `sortKey`).
    fn repair_groups(&mut self) {
        let missing: Vec<PresentationGroup> = self
            .projects
            .iter()
            .filter(|project| {
                !self
                    .groups
                    .iter()
                    .any(|group| group.project_id == project.project_id)
            })
            .map(|project| PresentationGroup {
                group_id: project.default_group_id(),
                project_id: project.project_id.clone(),
                session_ids: Vec::new(),
                sort_key: format!("{}:active", project.sort_key),
                title: "Active".to_string(),
            })
            .collect();
        self.groups.extend(missing);

        let incomplete: Vec<(String, String)> = self
            .sessions
            .iter()
            .filter(|(project_id, session_id, session)| {
                self.groups
                    .iter()
                    .find(|group| {
                        group.project_id == *project_id && group.group_id == session.group_id
                    })
                    .is_some_and(|group| !group.session_ids.iter().any(|id| id == session_id))
            })
            .map(|(project_id, _, session)| (project_id.to_string(), session.group_id.clone()))
            .collect();
        for (project_id, group_id) in incomplete {
            self.rebuild_group_session_ids(&project_id, &group_id);
        }
    }

    /// Rebuilds one group's display order from the sessions that name it. Returns whether the
    /// list changed.
    pub(super) fn rebuild_group_session_ids(&mut self, project_id: &str, group_id: &str) -> bool {
        let mut members: Vec<(&str, &str)> = self
            .sessions
            .project(project_id)
            .map(|sessions| {
                sessions
                    .values()
                    .filter(|session| session.group_id == group_id)
                    .map(|session| (session.sort_key.as_str(), session.session_id.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        members.sort_unstable();
        let next: Vec<String> = members
            .into_iter()
            .map(|(_, session_id)| session_id.to_string())
            .collect();
        match self
            .groups
            .iter_mut()
            .find(|group| group.project_id == project_id && group.group_id == group_id)
        {
            Some(group) if group.session_ids != next => {
                group.session_ids = next;
                true
            }
            _ => false,
        }
    }
}

/// CDXC:StateSync 2026-09-19 WHY:
/// The daemon orders rows by the byte order of `sortKey` (Rust `sort_by_key`), so the store does too. The TypeScript reducer used `localeCompare`, which can order punctuation and case differently from the daemon; following the daemon keeps a delta-built list equal to the list a fresh snapshot would give.
pub(super) fn sort_projects(projects: &mut [PresentationProject]) {
    projects.sort_by(|left, right| {
        (left.sort_key.as_str(), left.project_id.as_str())
            .cmp(&(right.sort_key.as_str(), right.project_id.as_str()))
    });
}

pub(super) fn sort_groups(groups: &mut [PresentationGroup]) {
    groups.sort_by(|left, right| {
        (left.sort_key.as_str(), left.group_id.as_str())
            .cmp(&(right.sort_key.as_str(), right.group_id.as_str()))
    });
}

/// The fields that decide whether a row is in a tab list at all.
pub(super) fn tab_listing(session: &PresentationSession) -> (bool, &SessionSurface, &SessionKind) {
    (
        session.visible_in_sidebar_by_default && session.side_chat_parent_session_id.is_none(),
        &session.surface,
        &session.kind,
    )
}

pub(super) fn project_id_order(projects: &[PresentationProject]) -> Vec<String> {
    projects
        .iter()
        .map(|project| project.project_id.clone())
        .collect()
}
