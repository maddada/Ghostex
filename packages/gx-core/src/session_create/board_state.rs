//! What the Project Board shows about conversations: the sessions a card can be linked to, and each
//! link's liveness, built the way `project-board.ts` built them (`createGpuiProjectBoardSessionOptions`,
//! `findGpuiProjectBoardLinkedSessionOption`, `createGpuiProjectBoardConversationState`).
//!
//! Pure: the host reads the projects, the presentation and the focus, performs the availability
//! checks a link without a live session needs, and hands the answers in.
//!
//! macOS `createProjectBoardConversationProjects` parity: a bead can be worked from a sibling
//! worktree while its ticket stays on the parent board, so the option list spans the worktree
//! family.
//!
//! CDXC:ProjectBoard 2026-08-07:
//! Rows that mount the same Beads board are part of the same board too. Their sessions belong in the
//! list, or a link inherited from one of them reads as dead while its session is still running. The
//! option list is scoped to the board's worktree family and board mounts, but a bead can be worked
//! from any project, so a link's liveness is resolved straight from the presentation.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/create/board.rs (the host).

use std::collections::{BTreeSet, HashMap};

use serde_json::{json, Map, Value};

use crate::keys::SessionKey;

use super::board_links::{board_session_id, BeadConversationLink};

/// A session of this computer, as the board reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardSession {
    pub project_id: String,
    pub session_id: String,
    pub title: String,
    /// `agentName ?? agentId`.
    pub agent_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub sleeping: bool,
}

/// The facts the options are built from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardSessionFacts {
    /// Agent and terminal sessions of this computer, in presentation order.
    pub sessions: Vec<BoardSession>,
    pub project_titles: HashMap<String, String>,
    pub active_project_id: Option<String>,
    /// The runtime's `focusedSessionId`: a raw session id.
    pub focused_session_id: Option<String>,
}

/// One option of the board's session picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardSessionOption {
    pub agent_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub is_focused: bool,
    pub is_sleeping: bool,
    pub label: String,
    pub session_id: String,
}

impl BoardSessionOption {
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        if let Some(agent_id) = &self.agent_id {
            map.insert("agentId".into(), json!(agent_id));
        }
        if let Some(agent_session_id) = &self.agent_session_id {
            map.insert("agentSessionId".into(), json!(agent_session_id));
        }
        map.insert("isFocused".into(), json!(self.is_focused));
        map.insert("isSleeping".into(), json!(self.is_sleeping));
        map.insert("label".into(), json!(self.label));
        map.insert("sessionId".into(), json!(self.session_id));
        Value::Object(map)
    }
}

fn worktree_parent(project: &Value) -> Option<String> {
    project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn project_id_of(project: &Value) -> &str {
    project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

impl BoardSessionFacts {
    fn option(&self, session: &BoardSession, board_project_id: &str, session_id: String) -> BoardSessionOption {
        let is_board = session.project_id == board_project_id;
        let label = if is_board {
            session.title.clone()
        } else {
            let project_title = self
                .project_titles
                .get(&session.project_id)
                .cloned()
                .unwrap_or_else(|| session.project_id.clone());
            format!("{project_title} · {}", session.title)
        };
        BoardSessionOption {
            agent_id: session.agent_id.clone(),
            agent_session_id: session.agent_session_id.clone(),
            is_focused: self.active_project_id.as_deref() == Some(session.project_id.as_str())
                && self.focused_session_id.as_deref() == Some(session.session_id.as_str()),
            is_sleeping: session.sleeping,
            label,
            session_id,
        }
    }

    /// `createGpuiProjectBoardSessionOptions(boardProject, linkStoreProjects)`.
    pub fn session_options(
        &self,
        board_project: &Value,
        link_store_projects: &[Value],
        projects: &[Value],
    ) -> Vec<BoardSessionOption> {
        let board_id = project_id_of(board_project);
        let family_parent = worktree_parent(board_project).unwrap_or_else(|| board_id.to_string());
        let mut related: BTreeSet<String> = BTreeSet::new();
        related.insert(board_id.to_string());
        for project in link_store_projects {
            related.insert(project_id_of(project).to_string());
        }
        for candidate in projects {
            let candidate_id = project_id_of(candidate);
            if candidate_id == family_parent
                || worktree_parent(candidate).as_deref() == Some(family_parent.as_str())
            {
                related.insert(candidate_id.to_string());
            }
        }
        self.sessions
            .iter()
            .filter(|session| related.contains(&session.project_id))
            .map(|session| {
                let session_id = board_session_id(&session.project_id, &session.session_id, board_id);
                self.option(session, board_id, session_id)
            })
            .collect()
    }

    /// `findGpuiProjectBoardLinkedSessionOption(boardProject, ghostexSessionId)`.
    pub fn linked_session_option(
        &self,
        board_project_id: &str,
        ghostex_session_id: &str,
    ) -> Option<BoardSessionOption> {
        let (project_id, session_id) = board_reference(ghostex_session_id, board_project_id);
        let session = self
            .sessions
            .iter()
            .find(|session| session.project_id == project_id && session.session_id == session_id)?;
        Some(self.option(session, board_project_id, ghostex_session_id.to_string()))
    }
}

/// A board session id back to `(projectId, sessionId)`: scoped ids name their project, a raw one
/// belongs to the board project.
pub fn board_reference(ghostex_session_id: &str, board_project_id: &str) -> (String, String) {
    match SessionKey::parse_sidebar_session_id(ghostex_session_id).filter(|key| key.machine.is_local()) {
        Some(key) => (key.project_id, key.session_id),
        None => (board_project_id.to_string(), ghostex_session_id.to_string()),
    }
}

/// What a link's availability check answered for a link with no live session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkAvailability {
    pub restorable: bool,
    pub resumable: bool,
    pub title: Option<String>,
}

/// One link as the board card reads it.
pub fn link_view(
    link: &BeadConversationLink,
    session: Option<&BoardSessionOption>,
    availability: Option<&LinkAvailability>,
) -> Value {
    let mut view = link.to_json();
    let Some(object) = view.as_object_mut() else {
        return view;
    };
    object.remove("agentId");
    if let Some(agent_id) = link
        .agent_id
        .clone()
        .or_else(|| session.and_then(|session| session.agent_id.clone()))
    {
        object.insert("agentId".into(), json!(agent_id));
    }
    if let Some(session) = session {
        object.insert("isFocused".into(), json!(session.is_focused));
        object.insert("isSleeping".into(), json!(session.is_sleeping));
    }
    object.insert("isLive".into(), json!(session.is_some()));
    object.insert(
        "isRestorable".into(),
        json!(availability.is_some_and(|availability| availability.restorable)),
    );
    object.insert(
        "isResumable".into(),
        json!(availability.is_some_and(|availability| availability.resumable)),
    );
    let title = session
        .map(|session| session.label.clone())
        .or_else(|| availability.and_then(|availability| availability.title.clone()));
    if let Some(title) = title {
        object.insert("sessionTitle".into(), json!(title));
    }
    view
}

/// The agents a board can start work with: the HUD's agents that have a command.
pub fn board_agent_options(hud: Option<&Value>) -> Vec<Value> {
    hud.and_then(|hud| hud.get("agents"))
        .and_then(Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .filter(|agent| {
                    agent
                        .get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|command| !command.trim().is_empty())
                })
                .map(|agent| {
                    let mut option = Map::new();
                    for (from, to) in [("agentId", "agentId"), ("command", "command"), ("icon", "icon"), ("name", "label")] {
                        if let Some(value) = agent.get(from).filter(|value| !value.is_null()) {
                            option.insert(to.into(), value.clone());
                        }
                    }
                    Value::Object(option)
                })
                .collect()
        })
        .unwrap_or_default()
}
