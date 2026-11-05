//! Which tracker a workspace's projects use for tickets and projects: Linear (tickets and Linear
//! projects), GitHub (issues and GitHub Projects), or the workspace's ticket command (its tickets,
//! picked only on this computer and never in a team workspace). Every caller asks `project_work_tracker` (or
//! `workspace_work_tracker`); the presentation projection reads the answer the background pass
//! remembered (`cached_project_work_tracker`).
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: "in settings we need to say what is the primary for that workspace (Linear Tickets &
//! Projects or Github Issues & Projects - Need to pick just 1)". The primary decides what Create
//! ticket makes, which tickets the Work page lists, what Link to offers, which project kind a card
//! shows, and where the Slack flow looks for (or creates) the ticket. A workspace that never picked
//! one uses Linear when a Linear key applies to it, otherwise GitHub. For a workspace connected to
//! a team the choice is a team flow setting in the team's Convex project (owners only), so every
//! teammate's Ghostex and the Slack flow agree.
//!
//! SEE-ALSO: `tracker` in the workspaces document (server/src/workspaces/store.rs) and in
//! packages/team-sync/convex/teamFlow.ts; `/api/readWorkTracker` and `/api/setWorkTracker` in
//! server/src/server/route_http/work_tracker.rs; `PresentationProject.work_tracker` in
//! packages/gx-protocol/src/presentation.rs.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;
use crate::workspaces::{project_workspace_id, workspace_exists, DEFAULT_WORKSPACE_ID};

use super::{linear_api_key, workspace_command, WorkSourceId};

/// How long a team's choice read from Convex is trusted before the background pass reads it again.
const TEAM_TRACKER_TTL: Duration = Duration::from_secs(5 * 60);

/// Where a workspace's tracker came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkTrackerSource {
    /// The team's flow settings in Convex.
    Team,
    /// Picked on this computer (Settings > Workspaces, `ghostex work-mode tracker`).
    Workspace,
    /// Never picked: Linear when a Linear key applies, otherwise GitHub.
    Default,
}

impl WorkTrackerSource {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Workspace => "workspace",
            Self::Default => "default",
        }
    }
}

struct TeamTracker {
    /// `None`: the team has not picked one.
    tracker: Option<WorkSourceId>,
    fetched_at: Instant,
}

/// The team's choice per workspace id, as Convex last answered.
fn team_trackers() -> &'static Mutex<HashMap<String, TeamTracker>> {
    static CACHE: OnceLock<Mutex<HashMap<String, TeamTracker>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The tracker the background pass found for each work-mode project, for the projection.
fn project_trackers() -> &'static Mutex<HashMap<String, WorkSourceId>> {
    static CACHE: OnceLock<Mutex<HashMap<String, WorkSourceId>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn remember_team_work_tracker(workspace_id: &str, tracker: Option<WorkSourceId>) {
    if let Ok(mut cache) = team_trackers().lock() {
        cache.insert(
            workspace_id.to_string(),
            TeamTracker {
                tracker,
                fetched_at: Instant::now(),
            },
        );
    }
}

fn forget_team_work_tracker(workspace_id: &str) {
    if let Ok(mut cache) = team_trackers().lock() {
        cache.remove(workspace_id);
    }
}

fn cached_team_work_tracker(workspace_id: &str) -> Option<WorkSourceId> {
    team_trackers()
        .lock()
        .ok()?
        .get(workspace_id)
        .and_then(|cached| cached.tracker)
}

/// Re-reads the team's choice from Convex when it is missing or stale. Blocking (a network call);
/// returns whether Convex was asked.
pub(crate) fn refresh_team_work_tracker(paths: &GxserverPaths, workspace_id: &str) -> bool {
    let fresh = team_trackers().lock().ok().is_some_and(|cache| {
        cache
            .get(workspace_id)
            .is_some_and(|cached| cached.fetched_at.elapsed() < TEAM_TRACKER_TTL)
    });
    // The team backend is part of Workspaces: with it off, a team's choice does not apply.
    if !crate::workspaces::workspaces_feature_enabled()
        || !crate::team_sync::workspace_has_team(paths, workspace_id)
    {
        // Left the team (or never joined): the team's choice no longer applies here.
        forget_team_work_tracker(workspace_id);
        return false;
    }
    if fresh {
        return false;
    }
    if let Ok(flow) = crate::team_sync::read_team_flow(paths, &workspace_params(workspace_id)) {
        remember_team_work_tracker(
            workspace_id,
            WorkSourceId::from_wire_built_in(flow.get("tracker")),
        );
    }
    true
}

pub(crate) fn workspace_params(workspace_id: &str) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("workspaceId".to_string(), json!(workspace_id));
    params
}

/// The tracker a workspace picked on this computer (the workspaces document), if any: Linear,
/// GitHub, or a ticket command's id this computer knows. Whether that command still applies is
/// the caller's to check (`workspace_work_tracker`).
pub(crate) fn stored_workspace_work_tracker(
    workspaces: &Value,
    workspace_id: &str,
) -> Option<WorkSourceId> {
    WorkSourceId::from_wire(
        workspaces
            .get("workspaces")
            .and_then(|workspaces| workspaces.get(workspace_id))
            .and_then(|workspace| workspace.get("tracker")),
    )
}

/// A workspace's tracker and where it came from: the team's choice, else this computer's, else
/// Linear when a Linear key applies to the workspace (its own key or the shared one), else GitHub.
pub(crate) fn workspace_work_tracker(
    paths: &GxserverPaths,
    workspaces: &Value,
    workspace_id: &str,
) -> (WorkSourceId, WorkTrackerSource) {
    let workspace_id = if workspace_exists(workspaces, workspace_id) {
        workspace_id
    } else {
        DEFAULT_WORKSPACE_ID
    };
    if let Some(tracker) = cached_team_work_tracker(workspace_id) {
        return (tracker, WorkTrackerSource::Team);
    }
    if let Some(tracker) = stored_workspace_work_tracker(workspaces, workspace_id)
        .filter(|tracker| command_tracker_applies(paths, workspace_id, *tracker))
    {
        return (tracker, WorkTrackerSource::Workspace);
    }
    let tracker = if linear_api_key(paths, None, Some(workspace_id)).is_some() {
        WorkSourceId::Linear
    } else {
        WorkSourceId::Github
    };
    (tracker, WorkTrackerSource::Default)
}

/// Whether a stored `tracker` applies: Linear and GitHub always do; a ticket command only while
/// the workspace pins it and has no team (a team's command is stopped), so a command that was
/// cleared or that a team join stopped leaves the workspace on the rule for one never picked.
fn command_tracker_applies(
    paths: &GxserverPaths,
    workspace_id: &str,
    tracker: WorkSourceId,
) -> bool {
    let WorkSourceId::Command(id) = tracker else {
        return true;
    };
    !crate::team_sync::workspace_has_team(paths, workspace_id)
        && workspace_command(workspace_id).is_some_and(|command| command.source_id == id.as_str())
}

/// Which tracker a project uses: its workspace's (a worktree project follows its parent checkout).
/// The one answer every caller uses: Create ticket, the Work page, Link to, the card chips and the
/// Slack flow's local half.
pub(crate) fn project_work_tracker(
    paths: &GxserverPaths,
    workspaces: &Value,
    project: &Value,
    projects: &[Value],
) -> WorkSourceId {
    let workspace_id = project_workspace_id(workspaces, project, projects);
    workspace_work_tracker(paths, workspaces, &workspace_id).0
}

/// Whether the workspace explicitly picked Linear as its tracker (the team's choice, else this
/// computer's), as opposed to Linear being the default for a never-picked workspace.
pub(crate) fn workspace_picked_linear(workspaces: &Value, workspace_id: &str) -> bool {
    let workspace_id = if workspace_exists(workspaces, workspace_id) {
        workspace_id
    } else {
        DEFAULT_WORKSPACE_ID
    };
    cached_team_work_tracker(workspace_id)
        .or_else(|| stored_workspace_work_tracker(workspaces, workspace_id))
        == Some(WorkSourceId::Linear)
}

/// The projects that may offer Create Linear Ticket, as the background pass found them.
///
/// CDXC:WorkMode 2026-11-05 DECISION:
/// User: "why do i see create linear ticket fam???" in a project with no Linear set up. The shared Linear key alone applies to every project, so it no longer shows the item: the workspace must have picked Linear as its tracker, or a key of the project's or its workspace's own must apply. Nothing else changes (the default tracker, the Work list and Link to read Linear as before). A set empty until the first pass, so the item never flashes at startup.
fn linear_create_projects() -> &'static Mutex<std::collections::HashSet<String>> {
    static CACHE: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

pub(crate) fn remember_project_linear_create(project_id: &str, applies: bool) {
    if let Ok(mut cache) = linear_create_projects().lock() {
        if applies {
            cache.insert(project_id.to_string());
        } else {
            cache.remove(project_id);
        }
    }
}

pub(crate) fn project_linear_create(project_id: &str) -> bool {
    linear_create_projects()
        .lock()
        .is_ok_and(|cache| cache.contains(project_id))
}

pub(crate) fn remember_project_work_tracker(project_id: &str, tracker: WorkSourceId) {
    if let Ok(mut cache) = project_trackers().lock() {
        cache.insert(project_id.to_string(), tracker);
    }
}

/// What the background pass found for this project; before its first pass, Linear when the pass
/// found a Linear key (the behaviour before trackers existed), otherwise GitHub.
pub(crate) fn cached_project_work_tracker(project_id: &str) -> WorkSourceId {
    if let Some(tracker) = project_trackers()
        .lock()
        .ok()
        .and_then(|cache| cache.get(project_id).copied())
    {
        return tracker;
    }
    if super::project_linear_key(project_id).is_some() {
        WorkSourceId::Linear
    } else {
        WorkSourceId::Github
    }
}

#[cfg(test)]
mod linear_create_tests {
    use super::*;

    #[test]
    fn only_an_explicit_linear_pick_counts_as_picked() {
        let doc = |tracker: Option<&str>| {
            let mut workspace = json!({});
            if let Some(tracker) = tracker {
                workspace["tracker"] = json!(tracker);
            }
            json!({ "workspaces": { "w-linear-create-test": workspace } })
        };
        assert!(workspace_picked_linear(&doc(Some("linear")), "w-linear-create-test"));
        assert!(!workspace_picked_linear(&doc(Some("github")), "w-linear-create-test"));
        assert!(!workspace_picked_linear(&doc(None), "w-linear-create-test"));
    }

    #[test]
    fn create_linear_ticket_is_off_until_the_pass_allows_it() {
        let id = "linear-create-test-project";
        assert!(!project_linear_create(id));
        remember_project_linear_create(id, true);
        assert!(project_linear_create(id));
        remember_project_linear_create(id, false);
        assert!(!project_linear_create(id));
    }
}
