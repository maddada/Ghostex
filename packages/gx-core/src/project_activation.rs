//! Opening a project from outside its sidebar rows (the menu bar, Quick Access, the Automate
//! board, a finished Add Project, a restored Recent Project, `ghostex switch-project`): which
//! session it lands on, or that it has none and one must be created.
//!
//! CDXC:Projects 2026-09-23 DECISION:
//! User: opening a project from Quick Access selects its last agent/terminal, or creates the default agent in Chat mode (a terminal when Terminal is the default), and focuses its input.
//! A sleeping last session is selected without waking it and shows its wake placeholder while Click to Wake Sleeping Panes is on; this supersedes the 2026-09-05 wording that woke it (CDXC:SessionSleep 2026-09-23 on `select_sleeping_local_workspace_tab`).
//!
//! CDXC:Projects 2026-09-05 DECISION:
//! User: remember each project's last selected agent or terminal across restarts, including sleeping sessions and closed projects.
//! The host keeps that record in client storage (`projectLastSession`) and hands the remembered row id in here; agent activity timestamps do not represent the user's selection.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/project_activation.rs (the host that performs the plan),
//! apps/desktop/src/app/gx_store/focus_perform.rs (the focus it ends in).

use std::collections::BTreeMap;

use ghostex_gx_protocol::{PresentationSession, PresentationSnapshot, SessionKind, SessionSurface};

use crate::keys::{ProjectKey, SessionKey};

/// What opening a project does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectActivation {
    /// Focus this session: the one the user last selected in the project when it still exists,
    /// else the project's first agent or terminal in the daemon's display order. `is_agent` picks
    /// the view it opens in (an agent opens in Chat, a terminal in its terminal).
    Focus { session: SessionKey, is_agent: bool },
    /// The project has no agent or terminal: make the project active and create one.
    Empty,
}

/// The plan, or `None` when the snapshot does not list the project.
///
/// `snapshot` is a fresh `/api/readPresentationSnapshot` of the project's machine, read the moment
/// the project is opened: a project restored from Recent Projects or added a moment ago is not in
/// the stream's rows yet, and its sessions must not look empty merely because their frames have not
/// arrived (the old runtime refreshed its copy the same way first).
///
/// The candidates are every agent and terminal the daemon groups of the project list, visible in
/// the sidebar by default and not in the command pane, in group order then display order. Locally
/// hidden sessions and user-made groups do not change the list: the old runtime read the daemon's
/// own groups (`createGxserverPresentationSessionsByProjectFromGroups` without its hidden sets).
pub fn plan_project_activation(
    snapshot: &PresentationSnapshot,
    project: &ProjectKey,
    remembered_row_id: Option<&str>,
) -> Option<ProjectActivation> {
    snapshot
        .projects
        .rows
        .iter()
        .find(|row| row.project_id == project.project_id)?;
    let sessions: BTreeMap<&str, &PresentationSession> = snapshot
        .sessions
        .rows
        .iter()
        .filter(|session| session.project_id == project.project_id)
        .map(|session| (session.session_id.as_str(), session))
        .collect();
    let candidates: Vec<(SessionKey, bool)> = snapshot
        .groups
        .rows
        .iter()
        .filter(|group| group.project_id == project.project_id)
        .flat_map(|group| group.session_ids.iter())
        .filter_map(|session_id| sessions.get(session_id.as_str()).copied())
        .filter(|session| {
            session.visible_in_sidebar_by_default && session.surface != SessionSurface::Commands
        })
        .filter_map(|session| {
            let is_agent = match session.kind {
                SessionKind::Agent => true,
                SessionKind::Terminal => false,
                _ => return None,
            };
            let key = SessionKey {
                machine: project.machine.clone(),
                project_id: project.project_id.clone(),
                session_id: session.session_id.clone(),
            };
            Some((key, is_agent))
        })
        .collect();
    let remembered = remembered_row_id.and_then(|row_id| {
        candidates
            .iter()
            .find(|(key, _)| key.to_sidebar_session_id() == row_id)
    });
    Some(match remembered.or_else(|| candidates.first()) {
        Some((session, is_agent)) => ProjectActivation::Focus {
            session: session.clone(),
            is_agent: *is_agent,
        },
        None => ProjectActivation::Empty,
    })
}

/// The client storage key of a project's remembered session: the catalog's `projectLastSession`
/// prefix and the project's workspace id (machine-scoped for a remote project).
pub fn project_last_session_storage_key(project: &ProjectKey) -> String {
    format!(
        "{PROJECT_LAST_SESSION_KEY_PREFIX}{}",
        project.to_workspace_project_id()
    )
}

/// `projectLastSession`'s `key` in `packages/client-storage/catalog.ts`.
pub const PROJECT_LAST_SESSION_KEY_PREFIX: &str = "ghostex.gpui.project-last-session.v1:";
