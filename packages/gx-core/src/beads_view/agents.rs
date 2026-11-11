//! Who is on a bead: the Ghostex sessions linked to it, and whether each is working right now.

use std::collections::HashMap;

use serde_json::Value;

use crate::session_create::{
    bead_conversation_link_match_key, board_reference, canonicalize_links_for_board,
    project_board_links, select_link_store_projects,
};

/// One session as the store knows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeadAgentSession {
    pub project_id: String,
    pub session_id: String,
    pub title: String,
    /// The agent's display name (`agentName`, else `agentId`).
    pub agent: Option<String>,
    /// The presentation's activity is `working`.
    pub working: bool,
    pub sleeping: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeadAgentState {
    Working,
    Idle,
    Sleeping,
    /// The linked session is gone from this computer.
    Gone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeadAgent {
    pub agent: String,
    pub session_title: String,
    pub project_id: String,
    pub session_id: String,
    pub state: BeadAgentState,
}

/// Agents by bead match key (the id after its prefix, so links survive a prefix rename).
pub type BeadAgents = HashMap<String, Vec<BeadAgent>>;

pub fn bead_agent_key(bead_id: &str) -> String {
    bead_conversation_link_match_key(bead_id)
}

/// CDXC:ProjectBoard 2026-11-11 WHY:
/// The board's links live on every project that mounts the same Beads folder, so a bead started from one project shows its agent in another; the links are gathered and re-expressed for this board exactly as the Board's own state does (`select_link_store_projects`, `canonicalize_links_for_board`). The green dot means the session's presentation activity is `working`, not merely that a linked session exists, which kept an agent that had finished reading as busy.
pub fn bead_agents(
    board_project: &Value,
    projects: &[Value],
    global_beads_directory: &str,
    now_iso: &str,
    sessions: &[BeadAgentSession],
) -> BeadAgents {
    let board_id = board_project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let store = select_link_store_projects(board_project, projects, global_beads_directory);
    let links: Vec<_> = store
        .iter()
        .flat_map(|project| project_board_links(project, now_iso))
        .filter(|link| link.status != "archived")
        .collect();
    let mut agents = BeadAgents::new();
    for link in canonicalize_links_for_board(&links, board_id) {
        let (project_id, session_id) = board_reference(&link.ghostex_session_id, board_id);
        let session = sessions
            .iter()
            .find(|session| session.project_id == project_id && session.session_id == session_id);
        let state = match session {
            None => BeadAgentState::Gone,
            Some(session) if session.sleeping => BeadAgentState::Sleeping,
            Some(session) if session.working => BeadAgentState::Working,
            Some(_) => BeadAgentState::Idle,
        };
        let agent = link
            .agent_name
            .clone()
            .or_else(|| session.and_then(|session| session.agent.clone()))
            .or_else(|| link.agent_id.clone())
            .or_else(|| {
                session
                    .map(|session| session.title.clone())
                    .filter(|title| !title.is_empty())
            })
            .unwrap_or_else(|| "Session".to_string());
        agents
            .entry(bead_agent_key(&link.bead_id))
            .or_default()
            .push(BeadAgent {
                agent: display_agent_name(&agent),
                session_title: session
                    .map(|session| session.title.clone())
                    .unwrap_or_default(),
                project_id,
                session_id,
                state,
            });
    }
    for list in agents.values_mut() {
        list.sort_by_key(|agent| match agent.state {
            BeadAgentState::Working => 0,
            BeadAgentState::Idle => 1,
            BeadAgentState::Sleeping => 2,
            BeadAgentState::Gone => 3,
        });
    }
    agents
}

/// `claude` → `Claude`; names that are already capitalized stay.
fn display_agent_name(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
