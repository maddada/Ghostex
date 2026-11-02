//! Read-only views over the presentation store. Every selector sees the effective state: daemon
//! rows with local overlays applied, locally hidden rows left out.

use std::borrow::Cow;

use ghostex_gx_protocol::{
    LifecycleState, PresentationProject, PresentationSession, SessionActivity, SessionKind,
    WorkspaceSessionGroup,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::focus::ActiveGroup;
use crate::keys::{MachineId, ProjectKey, SessionKey};
use crate::presentation_store::{LoadedPresentation, MachinePresentation, PresentationStore};

/// A value read from a machine's presentation.
///
/// CDXC:Workarea 2026-09-19 WHY:
/// Three different facts must never collapse into an empty list, because the desktop workspace reads an empty tab list as "this project has no sessions" and clears every restored tab, split, and session mapping. `NotLoaded`: the machine's first snapshot has not arrived. `Missing`: the machine is loaded but the thing asked about does not exist (a removed or locally hidden project, an unknown user-made group). `Loaded(vec![])` is left to mean exactly one thing: it exists and is empty.
#[derive(Clone, Debug, PartialEq)]
pub enum Loadable<T> {
    NotLoaded,
    Missing,
    Loaded(T),
}

impl<T> Loadable<T> {
    pub fn loaded(self) -> Option<T> {
        match self {
            Self::NotLoaded | Self::Missing => None,
            Self::Loaded(value) => Some(value),
        }
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Loadable<U> {
        match self {
            Self::NotLoaded => Loadable::NotLoaded,
            Self::Missing => Loadable::Missing,
            Self::Loaded(value) => Loadable::Loaded(map(value)),
        }
    }
}

/// Which neighbour of a tab to step to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabDirection {
    Next,
    Previous,
}

/// Synthetic project id of the Quick Automations overview row; never a workspace tab.
pub const QUICK_AUTOMATIONS_PROJECT_ID: &str = "quick-automations";
/// Title of a session that has none.
pub const DEFAULT_TERMINAL_SESSION_TITLE: &str = "Terminal Session";
/// Upper bound of a tab title, in UTF-16 code units like the JavaScript it replaces.
pub const TAB_SESSION_TITLE_MAX_UTF16: usize = 512;

/// One workspace tab, in the shape the tab strip draws.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TabSession {
    pub key: SessionKey,
    /// `displayTitle`, else `primaryTitle`, else `terminalTitle`, else `title`, else the default;
    /// trimmed and bounded.
    pub title: String,
    /// `terminal` or `agent`, never anything else: rows of any other kind are not tabs. The old
    /// bridge payload said `terminal` for both; see [`TabSession::bridge_kind`].
    pub kind: SessionKind,
    pub activity: SessionActivity,
    pub lifecycle_state: LifecycleState,
    pub is_sleeping: bool,
    pub is_draft: bool,
    pub is_generating_first_prompt_title: bool,
    /// The session's cwd, else the project folder.
    pub working_directory: Option<String>,
    /// The daemon's icon key, unresolved (the old runtime mapped it through the agent catalog).
    pub agent_icon: Option<String>,
    pub agent_name: Option<String>,
    pub agent_session_id: Option<String>,
    pub has_session_note: bool,
    pub stashed_prompt_count: Option<u64>,
    pub has_switchable_agents: bool,
}

impl PresentationStore {
    /// The effective session, or `None` when it does not exist, its machine is not loaded, or it
    /// is hidden locally.
    pub fn session(&self, key: &SessionKey) -> Option<Cow<'_, PresentationSession>> {
        self.machine(&key.machine)?
            .effective_session(&key.project_id, &key.session_id)
    }

    /// The project row, or `None` when it does not exist or is hidden locally.
    pub fn project(&self, key: &ProjectKey) -> Option<&PresentationProject> {
        let machine = self.machine(&key.machine)?;
        if machine.is_project_hidden(&key.project_id) {
            return None;
        }
        machine.loaded()?.project(&key.project_id)
    }

    pub fn project_of_session(&self, key: &SessionKey) -> Option<&PresentationProject> {
        self.session(key)?;
        self.project(&key.project_key())
    }

    /// Every effective session of a project on any surface, ordered by
    /// `(group_id, sort_key, session_id)`. `Missing` when the project does not exist or is hidden
    /// locally, so an empty list always means a project that has no sessions.
    pub fn sessions_of_project(
        &self,
        key: &ProjectKey,
    ) -> Loadable<Vec<Cow<'_, PresentationSession>>> {
        let Some(machine) = self.machine(&key.machine) else {
            return Loadable::NotLoaded;
        };
        let Some(loaded) = machine.loaded() else {
            return Loadable::NotLoaded;
        };
        if loaded.project(&key.project_id).is_none() || machine.is_project_hidden(&key.project_id) {
            return Loadable::Missing;
        }
        let mut sessions: Vec<Cow<'_, PresentationSession>> = loaded
            .project_sessions(&key.project_id)
            .into_iter()
            .flat_map(|sessions| sessions.keys())
            .filter_map(|session_id| machine.effective_session(&key.project_id, session_id))
            .collect();
        sessions.sort_by(|left, right| {
            (&left.group_id, &left.sort_key, &left.session_id).cmp(&(
                &right.group_id,
                &right.sort_key,
                &right.session_id,
            ))
        });
        Loadable::Loaded(sessions)
    }

    /// Resolves the raw local session id the focus state file holds (it does not name the
    /// project) or a remote-scoped id. `None` when no such session exists right now.
    pub fn resolve_focus_state_session_id(&self, value: &str) -> Option<SessionKey> {
        if let Some(remote) = SessionKey::parse_remote_scoped_session_id(value) {
            return Some(remote);
        }
        let loaded = self.loaded(&MachineId::Local)?;
        loaded
            .server_sessions()
            .find(|session| session.session_id == value)
            .map(|session| {
                SessionKey::local(session.project_id.as_str(), session.session_id.as_str())
            })
    }

    /// Whether the local daemon reports the session as running (with local patches applied).
    pub fn is_running_local_session(&self, project_id: &str, session_id: &str) -> bool {
        self.local_lifecycle(project_id, session_id) == Some(LifecycleState::Running)
    }

    /// Whether the local session is sleeping (with local patches applied).
    ///
    /// The TypeScript version also consulted two caches of the last published sidebar groups.
    /// Both were projections of this same presentation, so they add nothing here.
    pub fn is_sleeping_local_session(&self, project_id: &str, session_id: &str) -> bool {
        self.local_lifecycle(project_id, session_id) == Some(LifecycleState::Sleeping)
    }

    fn local_lifecycle(&self, project_id: &str, session_id: &str) -> Option<LifecycleState> {
        self.machine(&MachineId::Local)?
            .effective_session(project_id, session_id)
            .map(|session| session.lifecycle_state.clone())
    }

    /// The sidebar group a session's tab lives in: the user-made group that contains the session,
    /// else the project's own group.
    pub fn group_of_session(&self, key: &SessionKey) -> ActiveGroup {
        let project = key.project_key();
        match self
            .user_groups_of_project(&project)
            .iter()
            .find(|group| group.session_ids.contains(&key.session_id))
        {
            Some(group) => ActiveGroup::Subgroup {
                project,
                group_id: group.group_id.clone(),
            },
            None => ActiveGroup::Project(project),
        }
    }

    /// The ordered workspace tabs of a sidebar group.
    ///
    /// A project group lists the project's default-group sessions in display order, leaving out:
    /// ids without a row, rows with `visibleInSidebarByDefault` false, command-pane rows
    /// (`surface == commands`), locally hidden rows, and rows that belong to one of the project's
    /// user-made groups (those are the tabs of that group instead). A user-made group lists its
    /// member ids that still have a row, in its own order, with no further filter. The synthetic
    /// Quick Automations project is never a tab. Browser rows are host state, not sessions, and are
    /// not part of this list.
    ///
    /// `Missing` when the group's project does not exist or is hidden locally, or the user-made
    /// group is unknown.
    pub fn tab_sessions(&self, group: &ActiveGroup) -> Loadable<Vec<TabSession>> {
        let machine_id = group_machine(group);
        self.tab_refs(group).map(|refs| {
            let machine = self.machine(machine_id);
            refs.into_iter()
                .filter_map(|(project_id, session_id)| {
                    let machine = machine?;
                    tab_row(
                        machine_id,
                        machine,
                        machine.loaded()?,
                        project_id,
                        session_id,
                    )
                })
                .collect()
        })
    }

    /// The keys of [`Self::tab_sessions`], without building the rows.
    pub fn tab_session_keys(&self, group: &ActiveGroup) -> Loadable<Vec<SessionKey>> {
        let machine_id = group_machine(group);
        self.tab_refs(group).map(|refs| {
            refs.into_iter()
                .map(|(project_id, session_id)| SessionKey {
                    machine: machine_id.clone(),
                    project_id: project_id.to_string(),
                    session_id: session_id.to_string(),
                })
                .collect()
        })
    }

    /// The tab next to `from` in a group, wrapping at the ends. Made for a held "next tab" key: it
    /// builds no `TabSession` rows and clones no session. It does allocate one short-lived `Vec`
    /// of borrowed id pairs (16 bytes per tab) plus the returned key.
    ///
    /// When `from` is not a tab of the group, `Next` gives the first tab and `Previous` the last.
    /// `None` when the group has no tabs, is missing or not loaded, or `from` is its only tab.
    pub fn adjacent_tab_session(
        &self,
        group: &ActiveGroup,
        from: Option<&SessionKey>,
        direction: TabDirection,
    ) -> Option<SessionKey> {
        let machine_id = group_machine(group);
        let refs = self.tab_refs(group).loaded()?;
        let position = from
            .filter(|key| key.machine == *machine_id)
            .and_then(|key| {
                refs.iter().position(|(project_id, session_id)| {
                    *project_id == key.project_id && *session_id == key.session_id
                })
            });
        let target = match (position, direction) {
            (_, _) if refs.is_empty() => return None,
            (Some(_), _) if refs.len() == 1 => return None,
            (Some(index), TabDirection::Next) => (index + 1) % refs.len(),
            (Some(index), TabDirection::Previous) => (index + refs.len() - 1) % refs.len(),
            (None, TabDirection::Next) => 0,
            (None, TabDirection::Previous) => refs.len() - 1,
        };
        let (project_id, session_id) = refs.get(target)?;
        Some(SessionKey {
            machine: machine_id.clone(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        })
    }

    /// The ordered `(project_id, session_id)` pairs of a group's tabs, borrowed from the store.
    fn tab_refs<'a>(&'a self, group: &'a ActiveGroup) -> Loadable<Vec<(&'a str, &'a str)>> {
        let machine_id = group_machine(group);
        let Some(machine) = self.machine(machine_id) else {
            return Loadable::NotLoaded;
        };
        let Some(loaded) = machine.loaded() else {
            return Loadable::NotLoaded;
        };
        let project_exists = |project_id: &str| {
            loaded.project(project_id).is_some() && !machine.is_project_hidden(project_id)
        };
        let mut refs: Vec<(&str, &str)> = Vec::new();
        match group {
            ActiveGroup::Project(project) => {
                if !project_exists(&project.project_id) {
                    return Loadable::Missing;
                }
                let subgroups = self.user_groups_of_project(project);
                push_project_tab_refs(machine, loaded, &project.project_id, subgroups, &mut refs);
            }
            ActiveGroup::Subgroup { project, group_id } => {
                let subgroup = self
                    .user_groups_of_project(project)
                    .iter()
                    .find(|group| group.group_id == *group_id);
                let Some(subgroup) = subgroup.filter(|_| project_exists(&project.project_id))
                else {
                    return Loadable::Missing;
                };
                for session_id in &subgroup.session_ids {
                    push_tab_ref(machine, loaded, &project.project_id, session_id, &mut refs);
                }
            }
        }
        Loadable::Loaded(refs)
    }

    /// The user-made session groups of a project. They live in the LOCAL machine's workspace-groups
    /// document for every project, keyed by the workspace project id (raw for a local project,
    /// `remote:<machine>:project:<id>` for a remote one), because the desktop client owns them.
    pub fn user_groups_of_project(&self, project: &ProjectKey) -> &[WorkspaceSessionGroup] {
        self.machine(&MachineId::Local)
            .and_then(|machine| machine.side_state().workspace_groups.as_ref())
            .and_then(|state| state.projects.get(&project.to_workspace_project_id()))
            .map(|groups| groups.groups.as_slice())
            .unwrap_or_default()
    }

    /// The first project of a machine in sidebar order: where focus is re-homed when the active
    /// project goes away. `None` when the machine is not loaded or has none.
    pub fn first_project(&self, machine_id: &MachineId) -> Option<ProjectKey> {
        let machine = self.machine(machine_id)?;
        machine
            .loaded()?
            .projects()
            .iter()
            .filter(|project| !machine.is_project_hidden(&project.project_id))
            .min_by(|left, right| sidebar_project_order(machine, left, right))
            .map(|project| ProjectKey {
                machine: machine_id.clone(),
                project_id: project.project_id.clone(),
            })
    }
}

/// Sidebar order of two projects of one machine: by the manual project order (the machine's
/// workspace-groups `projectOrder`, keyed by raw project id) when either project is in it, else by
/// `sort_key`, then newest `updated_at`, then id.
fn sidebar_project_order(
    machine: &MachinePresentation,
    left: &PresentationProject,
    right: &PresentationProject,
) -> std::cmp::Ordering {
    let order = machine
        .side_state()
        .workspace_groups
        .as_ref()
        .map(|state| state.project_order.as_slice())
        .unwrap_or_default();
    let index = |project: &PresentationProject| {
        order
            .iter()
            .position(|candidate| *candidate == project.project_id)
    };
    match (index(left), index(right)) {
        (None, None) => left
            .sort_key
            .cmp(&right.sort_key)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.project_id.cmp(&right.project_id)),
        (left_index, right_index) => left_index
            .unwrap_or(usize::MAX)
            .cmp(&right_index.unwrap_or(usize::MAX)),
    }
}

fn group_machine(group: &ActiveGroup) -> &MachineId {
    match group {
        ActiveGroup::Project(project) | ActiveGroup::Subgroup { project, .. } => &project.machine,
    }
}

fn push_project_tab_refs<'a>(
    machine: &'a MachinePresentation,
    loaded: &'a LoadedPresentation,
    project_id: &'a str,
    subgroups: &[WorkspaceSessionGroup],
    refs: &mut Vec<(&'a str, &'a str)>,
) {
    if project_id == QUICK_AUTOMATIONS_PROJECT_ID || machine.is_project_hidden(project_id) {
        return;
    }
    // Ids are unique inside one group, so only a project with several groups (a legacy daemon)
    // needs the duplicate check; skipping it keeps a held "next tab" step linear in the tab count.
    let mut groups_seen = 0;
    for group in loaded
        .groups()
        .iter()
        .filter(|group| group.project_id == project_id)
    {
        groups_seen += 1;
        for session_id in &group.session_ids {
            let in_subgroup = subgroups
                .iter()
                .any(|subgroup| subgroup.session_ids.contains(session_id));
            let listed = loaded
                .server_session(project_id, session_id)
                .is_some_and(|session| session.listed_in_sidebar() && is_tab_kind(session));
            let duplicate = groups_seen > 1 && refs.contains(&(project_id, session_id.as_str()));
            if listed
                && !in_subgroup
                && !duplicate
                && !machine.is_session_hidden(project_id, session_id)
            {
                refs.push((project_id, session_id));
            }
        }
    }
}

/// The tab strip shows terminals and agents; any other kind a newer daemon adds is not a tab.
fn is_tab_kind(session: &PresentationSession) -> bool {
    matches!(session.kind, SessionKind::Terminal | SessionKind::Agent)
}

/// A member of a user-made group: listed when it still has a row, with no sidebar filter.
fn push_tab_ref<'a>(
    machine: &MachinePresentation,
    loaded: &'a LoadedPresentation,
    project_id: &'a str,
    session_id: &'a str,
    refs: &mut Vec<(&'a str, &'a str)>,
) {
    let is_tab = !machine.is_session_hidden(project_id, session_id)
        && loaded
            .server_session(project_id, session_id)
            .is_some_and(is_tab_kind);
    if is_tab && !refs.contains(&(project_id, session_id)) {
        refs.push((project_id, session_id));
    }
}

fn tab_row(
    machine_id: &MachineId,
    machine: &MachinePresentation,
    loaded: &LoadedPresentation,
    project_id: &str,
    session_id: &str,
) -> Option<TabSession> {
    let session = machine.effective_session(project_id, session_id)?;
    let working_directory = non_blank(session.cwd.as_deref())
        .or_else(|| {
            non_blank(
                loaded
                    .project(project_id)
                    .and_then(|project| project.path.as_deref()),
            )
        })
        .map(str::to_string);
    Some(TabSession {
        key: SessionKey {
            machine: machine_id.clone(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        },
        title: tab_title(&session),
        kind: session.kind.clone(),
        activity: session.activity.clone(),
        is_sleeping: session.lifecycle_state == LifecycleState::Sleeping,
        lifecycle_state: session.lifecycle_state.clone(),
        is_draft: session.is_draft,
        is_generating_first_prompt_title: session.is_generating_first_prompt_title,
        working_directory,
        agent_icon: session.agent_icon.clone(),
        agent_name: non_blank(session.agent_name.as_deref()).map(|name| name.trim().to_string()),
        agent_session_id: non_blank(session.agent_session_id.as_deref())
            .map(|id| id.trim().to_string()),
        has_session_note: non_blank(session.session_note.as_deref()).is_some(),
        stashed_prompt_count: session.stashed_prompt_count.filter(|count| *count > 0),
        has_switchable_agents: session
            .switchable_agents
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|agents| !agents.is_empty()),
    })
}

impl TabSession {
    /// The `kind` string of the old focus-state bridge payload, which is `terminal` for agents and
    /// terminals alike.
    ///
    /// The desktop contract parser (`AgentsWorkspaceSessionKind::from_sidebar_kind`) has one
    /// variant, maps both `agent` and `terminal` to it, and rejects the WHOLE payload as malformed
    /// for any other string. A host that still feeds that parser must send this value, never an
    /// unknown kind; the selector guarantees `kind` is one of the two.
    pub fn bridge_kind(&self) -> &'static str {
        "terminal"
    }

    /// The `activity` string of the old bridge payload. That parser also rejects the whole payload
    /// for an activity it does not know, and reads a missing one as idle; an activity value this
    /// client does not know is therefore reported as idle here, while `activity` keeps the real
    /// value.
    pub fn bridge_activity(&self) -> &'static str {
        match self.activity {
            SessionActivity::Working => "working",
            SessionActivity::Attention => "attention",
            SessionActivity::Idle | SessionActivity::Other(_) => "idle",
        }
    }
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn tab_title(session: &PresentationSession) -> String {
    // `primaryTitle ?? title`: only an absent primary title falls through to the stored title here.
    let primary = session
        .primary_title
        .as_deref()
        .unwrap_or(session.title.as_str());
    let title = non_blank(session.display_title.as_deref())
        .or_else(|| non_blank(Some(primary)))
        .or_else(|| non_blank(session.terminal_title.as_deref()))
        .or_else(|| non_blank(Some(session.title.as_str())))
        .unwrap_or(DEFAULT_TERMINAL_SESSION_TITLE)
        .trim();
    let mut bounded = String::new();
    let mut units = 0;
    for character in title.chars() {
        units += character.len_utf16();
        if units > TAB_SESSION_TITLE_MAX_UTF16 {
            break;
        }
        bounded.push(character);
    }
    bounded
}
