//! The Unified sidebar list: every session of every machine, Space and project, in one list.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::keys::{MachineId, ProjectKey, SessionKey};
use crate::sidebar_view::ordering::session_is_snoozed;
use crate::sidebar_view::spaces::space_claims_project;
use crate::sidebar_view::{
    GroupView, SessionRow, SessionView, SidebarView, SpacesState, LOCAL_MACHINE_ID, OTHER_SPACE_ID,
};

use super::repo::{
    merge_repo_identities, origin_keys_of, repo_identity, resolve_repo_identity, RepoIdentity,
};
use super::spaces::UnifiedSpaceScope;

/// One machine's rows as the Unified sidebar reads them.
pub struct UnifiedMachineInput<'a> {
    pub machine: MachineId,
    /// The machine's name as its tab spells it.
    pub label: String,
    /// That machine's list with no Space, tag filter, collapse or hide applied
    /// (`indicators::neutral_indicator_inputs`), so every session is in it.
    pub view: &'a SidebarView,
    /// The machine's Spaces as this window sees them, when its Spaces are on.
    pub spaces: Option<SpacesState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnifiedSettings {
    /// `unifiedWorkingSection`: working rows fold into the Working shelf.
    pub working_section: bool,
    /// `enableSessionParking`: parked rows sit in the Parked shelf.
    pub enable_session_parking: bool,
}

/// What the header's filter keeps. An empty set keeps everything of its kind; the kinds narrow
/// together (a row must pass every non-empty one).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedFilter {
    /// [`RepoIdentity::key`]s.
    pub repos: BTreeSet<String>,
    /// Machine ids (`local` for this computer).
    pub machines: BTreeSet<String>,
    /// [`space_filter_key`]s.
    pub spaces: BTreeSet<String>,
    /// The session text search, the filter bar's text chip. It narrows with the rest but is no pick
    /// of the filter menu, and it is not kept across restarts.
    pub query: String,
}

impl UnifiedFilter {
    /// No pick of the filter menu is set; the text search is separate.
    pub fn is_empty(&self) -> bool {
        self.repos.is_empty() && self.machines.is_empty() && self.spaces.is_empty()
    }

    /// Nothing narrows the list at all.
    pub fn keeps_everything(&self) -> bool {
        self.is_empty() && self.query.trim().is_empty()
    }

    /// The picks that still name a choice. A restored filter can name a project that is hidden now
    /// or a machine that is away, which would leave an empty list with nothing ticked to clear; it
    /// stays saved, so it is back when they are.
    pub fn within(&self, options: &UnifiedFilterOptions) -> Self {
        let kept = |picks: &BTreeSet<String>, options: &[UnifiedFilterOption]| {
            picks
                .iter()
                .filter(|pick| options.iter().any(|option| option.key == **pick))
                .cloned()
                .collect()
        };
        Self {
            repos: kept(&self.repos, &options.repos),
            machines: kept(&self.machines, &options.machines),
            spaces: kept(&self.spaces, &options.spaces),
            query: self.query.clone(),
        }
    }

    pub(super) fn keeps(&self, row: &UnifiedRow) -> bool {
        self.keeps_repo(row)
            && self.keeps_machine(row)
            && self.keeps_space(row)
            && query_matches(&self.query, row)
    }

    pub(super) fn keeps_repo(&self, row: &UnifiedRow) -> bool {
        self.repos.is_empty() || self.repos.contains(&row.repo.key)
    }

    pub(super) fn keeps_machine(&self, row: &UnifiedRow) -> bool {
        self.machines.is_empty() || self.machines.contains(machine_key(&row.machine))
    }

    pub(super) fn keeps_space(&self, row: &UnifiedRow) -> bool {
        self.spaces.is_empty()
            || row
                .space_key
                .as_ref()
                .is_some_and(|key| self.spaces.contains(key))
    }
}

/// Whether every word of the text search is in a row's title, project, branch or machine.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q854; where you type it moved with Q880, see native_sidebar/unified/filter_bar.rs): searching looks in the list itself: the title, the project and the branch, for a reporter with a hundred projects who could not find one in the filter menu. Each word has to match somewhere, in any order and any case, so "acme fix" finds the acme session whose title says fix. The machine's name counts too, so "studio" narrows to that computer. Searching runs after the filter menu's picks and over every shelf, so a snoozed session is findable.
fn query_matches(query: &str, row: &UnifiedRow) -> bool {
    let words = query.split_whitespace().map(str::to_lowercase);
    let session = &row.session.row;
    let branch = session.menu_facts.branch.as_deref().unwrap_or_default();
    let machine = row.machine_label.as_deref().unwrap_or_default();
    let haystack = [
        session.display_title.as_str(),
        row.repo.label.as_str(),
        branch,
        machine,
    ]
    .join("\n")
    .to_lowercase();
    words.into_iter().all(|word| haystack.contains(&word))
}

/// Where a row sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnifiedShelf {
    /// Sessions nobody has sent a message to yet ([`is_unified_draft`]).
    Drafts,
    Pinned,
    Active,
    Working,
    Snoozed,
    Parked,
}

/// The status word a card shows in its top right corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnifiedStatus {
    Approval,
    Question,
    Failed,
    Working,
    /// The turn ended but a background shell or monitor the agent started still runs.
    Waiting,
    /// A finished turn the user has not looked at.
    Done,
}

impl UnifiedStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approval => "Approval",
            Self::Question => "Question",
            Self::Failed => "Failed",
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Done => "Done",
        }
    }
}

/// The status a row shows, most urgent first, in the order the classic row's dots use.
pub fn unified_status(row: &SessionRow) -> Option<UnifiedStatus> {
    if row.awaiting_approval {
        Some(UnifiedStatus::Approval)
    } else if row.pending_question_count > 0 {
        Some(UnifiedStatus::Question)
    } else if row.model_selection_failed || row.program_status_failed || row.stuck {
        Some(UnifiedStatus::Failed)
    } else if row.activity == "working" {
        Some(UnifiedStatus::Working)
    } else if row.activity == "attention" {
        Some(UnifiedStatus::Done)
    } else if row.has_background_work {
        Some(UnifiedStatus::Waiting)
    } else {
        None
    }
}

/// Whether a card says its session works outside git: it has no branch and its project's folder is
/// in no git work tree.
///
/// CDXC:Sidebar 2026-11-04 DECISION:
/// User: "for threads on madda folder which doesn't have github it just shows 2 rows (it shouldn't do this, should say something that indicates that it's without source control for that thread, with an icon there too to indicate this (no need for different color tho)". The card's third line reads "Untracked" for such a session (User, Q1239: "'No source control' is way too long man"). The project's git probe (`gitRemoteOriginUrl`, `gitRepositoryRootPath`) decides, not the session's own `gitStatus`: gxserver probes a session's branch only while it is live, so a sleeping session in a repository has none. A session with a branch never says it, so a live session in a project the probe has not reached yet (the first minutes after its gxserver starts) shows its branch rather than the claim.
pub fn no_source_control(group: &GroupView, session: &SessionView) -> bool {
    session.row.menu_facts.branch.is_none()
        && group
            .core
            .project_context
            .as_ref()
            .is_some_and(|project| !project.git_work_tree)
}

/// One drawn row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedRow {
    pub machine: MachineId,
    /// The machine's name, for a row of another machine; `None` for this computer.
    pub machine_label: Option<String>,
    /// The group the row came from, which its menus and hover buttons are built against.
    pub group: GroupView,
    pub session: SessionView,
    pub repo: RepoIdentity,
    /// [`space_filter_key`] of the Space that shows the row's project; `None` when its machine
    /// has Spaces off.
    pub space_key: Option<String>,
    pub shelf: UnifiedShelf,
    pub status: Option<UnifiedStatus>,
    /// When the session last came back to the user ([`came_back_ms`]).
    pub came_back_ms: i64,
    /// [`no_source_control`]: the card says its folder is not under git.
    pub no_source_control: bool,
}

/// One choice of the filter menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedFilterOption {
    pub key: String,
    pub label: String,
    /// What the menu prints after the label: the machines a repository is on, or a Space's machine.
    pub detail: Option<String>,
    /// A repository's projects, one per machine and worktree, as sidebar group ids: what the
    /// row's Remove button takes out. Empty for a machine, a Space, and a user-made group with no
    /// project behind it.
    pub project_group_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedFilterOptions {
    pub repos: Vec<UnifiedFilterOption>,
    pub machines: Vec<UnifiedFilterOption>,
    pub spaces: Vec<UnifiedFilterOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnifiedSidebarView {
    pub drafts: Vec<UnifiedRow>,
    pub pinned: Vec<UnifiedRow>,
    pub active: Vec<UnifiedRow>,
    pub working: Vec<UnifiedRow>,
    pub snoozed: Vec<UnifiedRow>,
    pub parked: Vec<UnifiedRow>,
    pub options: UnifiedFilterOptions,
    /// Rows before the filter, so an empty list can tell "nothing here" from "nothing left"; inside
    /// the active Space while one scopes the list.
    pub total: usize,
}

impl UnifiedSidebarView {
    pub fn is_empty(&self) -> bool {
        self.drafts.is_empty()
            && self.pinned.is_empty()
            && self.active.is_empty()
            && self.working.is_empty()
            && self.snoozed.is_empty()
            && self.parked.is_empty()
    }

    /// The labels of the repositories the filter keeps, for the empty-list sentence.
    pub fn filtered_repo_labels(&self, filter: &UnifiedFilter) -> Vec<String> {
        self.options
            .repos
            .iter()
            .filter(|option| filter.repos.contains(&option.key))
            .map(|option| option.label.clone())
            .collect()
    }
}

/// The filter key of a Space: Space ids are unique per machine only.
pub fn space_filter_key(machine: &MachineId, space_id: &str) -> String {
    format!("{}|{space_id}", machine_key(machine))
}

pub(super) fn machine_key(machine: &MachineId) -> &str {
    machine.remote_id().unwrap_or(LOCAL_MACHINE_ID)
}

/// When a session last came back to the user: started, a turn begun, or a turn ended.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User (Q790): the Unified list is ordered by when a session last came back to you, so output while an agent works never moves a row. While the agent works that moment is the stint's start (`workingStartedAt`); once it stops it is the session's meaningful-activity clock, which gxserver last advanced when the turn ended or asked for the user (server/src/session_status/model.rs). Either way the creation time is the floor.
pub fn came_back_ms(row: &SessionRow) -> i64 {
    let created = row.timing.created_ms.unwrap_or(0);
    let moment = if row.activity == "working" {
        row.timing
            .working_started_ms
            .or(row.timing.last_interaction_ms)
    } else {
        row.timing.last_interaction_ms
    };
    created.max(moment.unwrap_or(0))
}

/// Whether a row is a draft card: a session nobody has sent a message to yet (gxserver's
/// `draftStatus`, the pencil on its row), whether or not its chat box holds text.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: "when a session is like this one in the unified sidebar then it should be placed in a new area at the top of the sidebar called "Drafts"", pointing at an empty pencil card ("Claude Session"; a draft holding text is retitled from that text, so that card had none). Every never-sent session is a draft card, typed into or not, as in the Project sidebar's Drafts section since CDXC:Drafts 2026-11-02 (sidebar_view/ordering.rs). The Drafts shelf sits at the very top, above Pinned, open, newest first, and is left out when empty. A pinned, snoozed or parked draft stays in its own place, as in the Project sidebar. The first message sent clears the marker and the session joins the list by the usual order; typing alone never moves a card.
pub fn is_unified_draft(row: &SessionRow) -> bool {
    row.is_draft
}

/// The shelf of a row.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User (Q791, Q792): working sessions fold into a Working shelf at the bottom only while Group working sessions is on, and parking stays manual: the Parked shelf is the classic Parked section, so it follows Enable session parking.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: "i often see the sidebar scroll by itself when i activate another session", asking that the cards under the pointer never jump. Which session is open no longer decides a shelf: the Project sidebar keeps the open working session in Sessions, but here that pulled the newly opened card out of the Working shelf and dropped the previously open one into it, reshuffling the list on every activation. A working card stays in the Working shelf while it is open. This supersedes the open-session exception of the Q791 note above.
///
/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: the Unified sidebar gets its own "Working section" switch in Settings › Sidebar & Sessions › Sidebar, shown while it is selected and on by default, instead of following Group working sessions: "Fold sessions whose agent is working or still running something in the background into a Working section at the bottom of the list. They return to the top of the list when they need you." A session is on the shelf while its status is Working, or Waiting (its turn ended but a shell or monitor it started still runs); the moment it asks a question, needs an approval, fails or finishes, that status outranks Working (`unified_status`) and it leaves the shelf for the list, which orders it by that return. This supersedes Q791's link to Group working sessions.
pub fn unified_shelf(
    row: &SessionView,
    settings: UnifiedSettings,
    draft: bool,
    now_ms: u64,
) -> UnifiedShelf {
    let session = &row.row;
    if session_is_snoozed(session.timing.snoozed_until_ms, now_ms) {
        UnifiedShelf::Snoozed
    } else if settings.enable_session_parking && session.is_parked {
        UnifiedShelf::Parked
    } else if session.is_pinned {
        UnifiedShelf::Pinned
    } else if draft {
        UnifiedShelf::Drafts
    } else if settings.working_section
        && matches!(
            unified_status(session),
            Some(UnifiedStatus::Working | UnifiedStatus::Waiting)
        )
    {
        UnifiedShelf::Working
    } else {
        UnifiedShelf::Active
    }
}

/// The Space that shows a group's project on its machine: the first Space in order that claims
/// it, else Other.
fn space_of_group(state: &SpacesState, group: &GroupView) -> String {
    let Some(project) = &group.space_project else {
        return OTHER_SPACE_ID.to_string();
    };
    state
        .ordered()
        .into_iter()
        .find(|space| {
            space_claims_project(
                space,
                &project.project_id,
                project.collection_id.as_deref(),
                project.parent_project_id.as_deref(),
            )
        })
        .map(|space| space.space_id.clone())
        .unwrap_or_else(|| OTHER_SPACE_ID.to_string())
}

/// The Space that shows `project_id` in `view`, its machine's list ([`space_of_group`]); `None`
/// when no group of that list is the project.
pub fn unified_space_of_project(
    view: &SidebarView,
    spaces: &SpacesState,
    project_id: &str,
) -> Option<String> {
    view.groups
        .iter()
        .find(|group| {
            group
                .space_project
                .as_ref()
                .map(|project| project.project_id.as_str())
                .or_else(|| {
                    group
                        .core
                        .project_context
                        .as_ref()
                        .map(|context| context.project_id.as_str())
                })
                == Some(project_id)
        })
        .map(|group| space_of_group(spaces, group))
}

/// Every group's repository, a user-made session group taking its project's.
fn repos_of(view: &SidebarView) -> BTreeMap<String, RepoIdentity> {
    let mut by_project: BTreeMap<String, RepoIdentity> = BTreeMap::new();
    for group in &view.groups {
        let Some(context) = &group.core.project_context else {
            continue;
        };
        by_project.insert(
            context.project_id.clone(),
            repo_identity(
                context.git_remote_origin_url.as_deref(),
                &context.path,
                context
                    .worktree
                    .as_ref()
                    .map(|worktree| worktree.parent_project_path.as_str()),
                &group.core.title,
            ),
        );
    }
    by_project
}

/// Every machine's project repositories, merged across the machines (`merge_repo_identities`),
/// in the order of `views`.
fn merged_repos_of(views: &[&SidebarView]) -> Vec<BTreeMap<String, RepoIdentity>> {
    let raw: Vec<BTreeMap<String, RepoIdentity>> =
        views.iter().map(|view| repos_of(view)).collect();
    let merged = merge_repo_identities(raw.iter().flat_map(|repos| repos.values()));
    raw.into_iter()
        .map(|repos| {
            let origin_keys_here = origin_keys_of(repos.values(), &merged);
            repos
                .into_iter()
                .map(|(project_id, identity)| {
                    let resolved = resolve_repo_identity(&identity, &merged, &origin_keys_here);
                    (project_id, resolved)
                })
                .collect()
        })
        .collect()
}

/// The repository of a project, as the list groups it across `views`, for setting the filter to
/// it from elsewhere (a Quick Access project pick).
pub fn repo_of_project(views: &[&SidebarView], project_id: &str) -> Option<RepoIdentity> {
    merged_repos_of(views)
        .into_iter()
        .find_map(|mut repos| repos.remove(project_id))
}

/// A repository while the filter's options are gathered, across every machine.
#[derive(Default)]
struct RepoOption {
    label: String,
    machine_labels: BTreeSet<String>,
    project_group_ids: BTreeSet<String>,
}

/// The sessions a coordinator in the list started (its workers), at any depth, wherever they run.
///
/// CDXC:Sidebar 2026-11-04 DECISION:
/// User: "in the unified sidebar, i still see worker threads showing there even tho i think we agreed we shouldn't show those??". The Unified list shows a coordinator's card and none of its workers, on this computer or another: they are reached from the Workers list in the coordinator's chat and from the Project sidebar, which nests them under it, and the coordinator's card carries the crew badge the Project sidebar's coordinator row draws (working, waiting or approval count). A worker is known by the link the Project sidebar nests it by (`coordinator_parent`); a worker on another computer carries none on its own computer's row, so the copy its coordinator's computer borrows (`is_borrowed_thread`) names it. A worker shows again as its own card only when its coordinator is not in any machine's list (closed, or its machine gone), so a session the user started themselves, which has no link, is never left out. Shelf counts, the filter's total and the project headers count cards, so they leave workers out too; the computer tabs, Quick Access and the Running Agents panel count sessions, workers included, as they do with the Project sidebar.
fn worker_keys(machines: &[UnifiedMachineInput<'_>]) -> HashSet<SessionKey> {
    let mut parents: HashMap<&SessionKey, &SessionKey> = HashMap::new();
    let mut present: HashSet<&SessionKey> = HashSet::new();
    for machine in machines {
        for group in &machine.view.groups {
            for session in &group.core.sessions {
                let row = &session.row;
                let Some(key) = row.key.as_ref() else {
                    continue;
                };
                if let Some(parent) = row.coordinator_parent.as_ref() {
                    parents.insert(key, parent);
                }
                if !row.is_browser && !row.is_borrowed_thread() {
                    present.insert(key);
                }
            }
        }
    }
    // A loop (two coordinators that are each other's worker) never reaches a top coordinator; its
    // rows stay cards, as the Project sidebar keeps them top-level.
    let reaches_top = |key: &SessionKey| {
        let mut at = key;
        for _ in 0..=parents.len() {
            match parents.get(at) {
                Some(parent) if *parent == key => return false,
                Some(parent) => at = parent,
                None => return true,
            }
        }
        false
    };
    parents
        .iter()
        .filter(|(key, parent)| present.contains(*parent) && reaches_top(key))
        .map(|(key, _)| (*key).clone())
        .collect()
}

/// Builds the list.
pub fn build_unified_sidebar(
    machines: &[UnifiedMachineInput<'_>],
    settings: UnifiedSettings,
    filter: &UnifiedFilter,
    now_ms: u64,
) -> UnifiedSidebarView {
    build_unified_sidebar_scoped(machines, settings, filter, None, now_ms)
}

/// [`build_unified_sidebar`] with the list scoped to the active Space (`unified_space_scope`).
///
/// CDXC:Spaces 2026-11-05 DECISION:
/// The active Space scopes the list and the header's filter narrows inside it. Only the scope's machine is scoped: its projects outside the Space leave the list, the filter's Projects and Remove, the shelves, Drafts and the total, as if that machine had only the Space's projects, so an empty Space says so instead of "nothing matches". That machine's Spaces leave the filter menu, because the Space row above the list is how one is picked now and a second Space pick could only empty the list; a saved pick of one stays saved and narrows again once Spaces is off. Other machines are not scoped: their Spaces are theirs, no row on screen shows which one they are on, and hiding their sessions would leave the Unified sidebar no way to reach them, so they stay in every Space with their machine's name on the card, and their Spaces stay in the filter menu to narrow them. That is Separate by machine only: in the one list (the default since 2026-11-05) the Spaces are shared by every machine (`project_docs::shared`), so every machine's cards are scoped by the one active Space, and no machine's Spaces stay in the filter menu. Drafts are scoped like any card: a draft is a session of a project, and it shows in its project's Space.
pub fn build_unified_sidebar_scoped(
    machines: &[UnifiedMachineInput<'_>],
    settings: UnifiedSettings,
    filter: &UnifiedFilter,
    scope: Option<&UnifiedSpaceScope>,
    now_ms: u64,
) -> UnifiedSidebarView {
    let mut rows: Vec<UnifiedRow> = Vec::new();
    let mut repo_machines: BTreeMap<String, RepoOption> = BTreeMap::new();
    let mut options = UnifiedFilterOptions::default();
    let views: Vec<&SidebarView> = machines.iter().map(|machine| machine.view).collect();
    let mut merged_repos = merged_repos_of(&views).into_iter();
    let workers = worker_keys(machines);
    for machine in machines {
        options.machines.push(UnifiedFilterOption {
            key: machine_key(&machine.machine).to_string(),
            label: machine.label.clone(),
            detail: None,
            ..UnifiedFilterOption::default()
        });
        let scoped = scope.filter(|scope| {
            (scope.every_machine || scope.machine == machine.machine) && machine.spaces.is_some()
        });
        if let Some(spaces) = machine.spaces.as_ref().filter(|_| scoped.is_none()) {
            let remote = (!machine.machine.is_local()).then(|| machine.label.clone());
            for space in spaces.ordered() {
                options.spaces.push(UnifiedFilterOption {
                    key: space_filter_key(&machine.machine, &space.space_id),
                    label: space.name.clone(),
                    detail: remote.clone(),
                    ..UnifiedFilterOption::default()
                });
            }
            options.spaces.push(UnifiedFilterOption {
                key: space_filter_key(&machine.machine, OTHER_SPACE_ID),
                label: "Other".to_string(),
                detail: remote,
                ..UnifiedFilterOption::default()
            });
        }
        let repos = merged_repos.next().unwrap_or_default();
        let machine_label = (!machine.machine.is_local()).then(|| machine.label.clone());
        for group in &machine.view.groups {
            // A hidden collection's projects are still in `groups`; a hidden project is not.
            if machine
                .view
                .hidden_collection_group_ids
                .contains(&group.core.group_id)
            {
                continue;
            }
            let space_id = machine
                .spaces
                .as_ref()
                .map(|spaces| space_of_group(spaces, group));
            if let Some(scope) = scoped {
                // Another machine's copy of a shared Space is the same Space by id, else by name
                // before the shared pass has given it the same id (`project_docs::shared`).
                let same_space = |space_id: &str| {
                    space_id == scope.space_id
                        || (scope.every_machine
                            && scope.space_id != OTHER_SPACE_ID
                            && machine.spaces.as_ref().is_some_and(|spaces| {
                                spaces.spaces.get(space_id).is_some_and(|space| {
                                    space.name.trim().eq_ignore_ascii_case(scope.label.trim())
                                })
                            }))
                };
                if !space_id.as_deref().is_some_and(same_space) {
                    continue;
                }
            }
            let project_id = group
                .space_project
                .as_ref()
                .map(|project| project.project_id.as_str())
                .or_else(|| {
                    group
                        .core
                        .project_context
                        .as_ref()
                        .map(|context| context.project_id.as_str())
                });
            let repo = project_id
                .and_then(|project_id| repos.get(project_id).cloned())
                .unwrap_or_else(|| repo_identity(None, "", None, &group.core.title));
            let entry = repo_machines
                .entry(repo.key.clone())
                .or_insert_with(|| RepoOption {
                    label: repo.label.clone(),
                    ..RepoOption::default()
                });
            entry.machine_labels.insert(machine.label.clone());
            if let Some(project_id) = project_id {
                entry.project_group_ids.insert(
                    ProjectKey {
                        machine: machine.machine.clone(),
                        project_id: project_id.to_string(),
                    }
                    .to_sidebar_group_id(),
                );
            }
            let space_key = space_id
                .as_ref()
                .map(|space_id| space_filter_key(&machine.machine, space_id));
            for session in &group.core.sessions {
                // A coordinator's worker is no card of its own (`worker_keys`).
                if session.row.is_browser
                    || session.row.is_borrowed_thread()
                    || session
                        .row
                        .key
                        .as_ref()
                        .is_none_or(|key| workers.contains(key))
                {
                    continue;
                }
                rows.push(UnifiedRow {
                    machine: machine.machine.clone(),
                    machine_label: machine_label.clone(),
                    group: group.clone(),
                    session: session.clone(),
                    repo: repo.clone(),
                    space_key: space_key.clone(),
                    shelf: unified_shelf(session, settings, is_unified_draft(&session.row), now_ms),
                    status: unified_status(&session.row),
                    came_back_ms: came_back_ms(&session.row),
                    no_source_control: no_source_control(group, session),
                });
            }
        }
    }
    options.repos = repo_machines
        .into_iter()
        .map(|(key, repo)| UnifiedFilterOption {
            key,
            label: repo.label,
            detail: (repo.machine_labels.len() > 1).then(|| {
                repo.machine_labels
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
            project_group_ids: repo.project_group_ids.into_iter().collect(),
        })
        .collect();
    options
        .repos
        .sort_by(|left, right| left.label.to_lowercase().cmp(&right.label.to_lowercase()));
    // Two different repositories with the same name (a fork, another owner's, a folder with no
    // remote) say which is which.
    let names: Vec<String> = options
        .repos
        .iter()
        .map(|option| option.label.to_lowercase())
        .collect();
    for (index, option) in options.repos.iter_mut().enumerate() {
        let shared = names
            .iter()
            .enumerate()
            .any(|(other, name)| other != index && *name == names[index]);
        if !shared {
            continue;
        }
        let which = repo_disambiguator(&option.key);
        option.detail = Some(match option.detail.take() {
            Some(machines) => format!("{which} · {machines}"),
            None => which,
        });
    }

    // One repository reads the same on every machine: the label this computer spells it with,
    // else the first machine's.
    for row in &mut rows {
        if let Some(option) = options
            .repos
            .iter()
            .find(|option| option.key == row.repo.key)
        {
            row.repo.label.clone_from(&option.label);
        }
    }
    let total = rows.len();
    let mut view = UnifiedSidebarView {
        options,
        total,
        ..Default::default()
    };
    for row in rows.into_iter().filter(|row| filter.keeps(row)) {
        match row.shelf {
            UnifiedShelf::Drafts => view.drafts.push(row),
            UnifiedShelf::Pinned => view.pinned.push(row),
            UnifiedShelf::Active => view.active.push(row),
            UnifiedShelf::Working => view.working.push(row),
            UnifiedShelf::Snoozed => view.snoozed.push(row),
            UnifiedShelf::Parked => view.parked.push(row),
        }
    }
    let by_came_back = |left: &UnifiedRow, right: &UnifiedRow| {
        right
            .came_back_ms
            .cmp(&left.came_back_ms)
            .then_with(|| tie_break(left, right))
    };
    // The newest draft first.
    view.drafts.sort_by(|left, right| {
        let created = |row: &UnifiedRow| row.session.row.timing.created_ms.unwrap_or(0);
        created(right)
            .cmp(&created(left))
            .then_with(|| tie_break(left, right))
    });
    view.pinned.sort_by(by_came_back);
    view.active.sort_by(by_came_back);
    // The Working shelf lists the session you last sent work to first.
    view.working.sort_by(|left, right| {
        let started = |row: &UnifiedRow| row.session.row.timing.working_started_ms.unwrap_or(0);
        started(right)
            .cmp(&started(left))
            .then_with(|| tie_break(left, right))
    });
    // The soonest to wake first.
    view.snoozed.sort_by(|left, right| {
        let wake = |row: &UnifiedRow| row.session.row.timing.snoozed_until_ms.unwrap_or(i64::MAX);
        wake(left)
            .cmp(&wake(right))
            .then_with(|| tie_break(left, right))
    });
    // Parked rows read latest active first, as everywhere else (CDXC:Sessions 2026-09-12 in
    // sidebar_view/ordering.rs).
    view.parked.sort_by(|left, right| {
        let active = |row: &UnifiedRow| row.session.row.timing.last_interaction_ms.unwrap_or(0);
        active(right)
            .cmp(&active(left))
            .then_with(|| tie_break(left, right))
    });
    view
}

/// `owner/repo` for a repository with a remote, `no remote` for a folder.
pub(super) fn repo_disambiguator(key: &str) -> String {
    if key.starts_with("dir:") {
        return "no remote".to_string();
    }
    let parts: Vec<&str> = key.split('/').collect();
    match parts.as_slice() {
        [.., owner, repo] => format!("{owner}/{repo}"),
        _ => key.to_string(),
    }
}

fn tie_break(left: &UnifiedRow, right: &UnifiedRow) -> std::cmp::Ordering {
    left.session
        .row
        .sidebar_session_id
        .cmp(&right.session.row.sidebar_session_id)
}
