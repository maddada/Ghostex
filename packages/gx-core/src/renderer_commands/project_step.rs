//! `ghostex move-project --direction up|down`: one step in this computer's sidebar project order.
//!
//! The step is expressed as the drag the sidebar already performs (`moveGroup` onto the neighbour,
//! before it or after it), so the order, the Project Group membership and the documents it writes
//! are exactly a drag's. The neighbour is the next project of the same kind: a main project steps
//! over main projects (its worktrees travel with it), a worktree over the other worktrees of its
//! own project, and a chat project over chat projects.

use serde_json::Value;

use super::errors::RendererCommandError;
use crate::core::Core;
use crate::keys::{MachineId, ProjectKey};
use crate::sidebar_drag::sidebar_project_group_order;
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::SidebarInputs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectStepDirection {
    Up,
    Down,
}

/// The `moveGroup` drag a step is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectStep {
    pub group_id: String,
    pub target_group_id: String,
    /// `before` for up, `after` for down.
    pub position: &'static str,
}

impl ProjectStep {
    /// The sidebar command the drag posts.
    pub fn to_command(&self) -> Value {
        serde_json::json!({
            "type": "moveGroup",
            "groupId": self.group_id,
            "targetGroupId": self.target_group_id,
            "position": self.position,
        })
    }
}

/// `Ok(None)` when the project is already first or last among its peers; "No matching project"
/// when this computer's sidebar does not list it.
pub fn plan_project_step(
    core: &Core,
    inputs: &SidebarInputs,
    project_id: &str,
    direction: ProjectStepDirection,
) -> Result<Option<ProjectStep>, RendererCommandError> {
    let order =
        sidebar_project_group_order(core, inputs).ok_or(RendererCommandError::NoMatchingProject)?;
    let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
    let Some(machine) = core.presentation().machine(&MachineId::Local) else {
        return Err(RendererCommandError::NoMatchingProject);
    };
    let kind = |project_id: &str| {
        let parent = machine
            .domain_project(project_id)
            .and_then(|row| row.get("worktree"))
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(Value::as_str)
            .map(js_trim)
            .filter(|parent| !parent.is_empty())
            .map(str::to_string);
        (machine.is_chat_project(project_id), parent)
    };
    let own_kind = kind(project_id);
    let peers: Vec<&String> = order
        .iter()
        .filter(|candidate| {
            ProjectKey::parse_sidebar_group_id(candidate)
                .filter(|key| key.machine.is_local())
                .is_some_and(|key| kind(&key.project_id) == own_kind)
        })
        .collect();
    let index = peers
        .iter()
        .position(|candidate| **candidate == group_id)
        .ok_or(RendererCommandError::NoMatchingProject)?;
    let (neighbour, position) = match direction {
        ProjectStepDirection::Up => match index.checked_sub(1) {
            Some(previous) => (peers[previous], "before"),
            None => return Ok(None),
        },
        ProjectStepDirection::Down => match peers.get(index + 1) {
            Some(next) => (*next, "after"),
            None => return Ok(None),
        },
    };
    Ok(Some(ProjectStep {
        group_id,
        target_group_id: neighbour.clone(),
        position,
    }))
}
