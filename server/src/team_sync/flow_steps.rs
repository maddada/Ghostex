//! A Work workspace's team-flow steps kept in its team's Convex project (`teamFlowSteps.ts`), for
//! crate::work_mode::team_flow, which keeps the local file for workspaces without a team.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: team flow settings are "Owners only". For a workspace connected to a team the steps are
//! one list shared by the whole team, read by every teammate's Work page and Settings and changed
//! only by owners; a team that has no steps yet uses the default flow.
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! The Work page reads the steps for every ticket it opens, so they are kept for a short while
//! instead of asking the team's Convex project each time; a save here replaces what is kept.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::paths::GxserverPaths;

use super::connections::{read_team_connection, TeamConnection};
use super::convex_http::ConvexCallKind;
use super::operations::member_call;

const STEPS_FRESH_FOR: Duration = Duration::from_secs(30);

/// The team's steps (`None` while the team has none) and whether this member may change them.
#[derive(Clone, Debug)]
pub(crate) struct TeamFlowSteps {
    pub(crate) steps: Option<Value>,
    pub(crate) can_edit: bool,
}

fn kept() -> &'static Mutex<HashMap<String, (Instant, TeamFlowSteps)>> {
    static KEPT: OnceLock<Mutex<HashMap<String, (Instant, TeamFlowSteps)>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn keep(workspace_id: &str, steps: &TeamFlowSteps) {
    if let Ok(mut kept) = kept().lock() {
        kept.insert(workspace_id.to_string(), (Instant::now(), steps.clone()));
    }
}

fn parse(answer: &Value) -> TeamFlowSteps {
    TeamFlowSteps {
        steps: answer
            .get("steps")
            .filter(|steps| steps.as_array().is_some_and(|steps| !steps.is_empty()))
            .cloned(),
        can_edit: answer.get("canEdit").and_then(Value::as_bool) == Some(true),
    }
}

/// The workspace's team connection, while the Workspaces built-in extension is on.
fn team_connection(paths: &GxserverPaths, workspace_id: &str) -> Option<TeamConnection> {
    if !crate::workspaces::workspaces_feature_enabled() {
        return None;
    }
    read_team_connection(paths, workspace_id)
}

/// The team's steps for a workspace connected to a team; `None` when the workspace has no team.
pub(crate) fn read_team_flow_steps(
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Option<Result<TeamFlowSteps, String>> {
    let connection = team_connection(paths, workspace_id)?;
    if let Some((_, steps)) = kept()
        .lock()
        .ok()
        .and_then(|kept| kept.get(workspace_id).cloned())
        .filter(|(at, _)| at.elapsed() < STEPS_FRESH_FOR)
    {
        return Some(Ok(steps));
    }
    Some(
        member_call(
            &connection,
            ConvexCallKind::Query,
            "teamFlowSteps:get",
            Map::new(),
        )
        .map(|answer| {
            let steps = parse(&answer);
            keep(workspace_id, &steps);
            steps
        }),
    )
}

/// Saves the team's steps (already checked by `validate_team_flow_steps`), or with `None` removes
/// them so the team uses the default flow. `None` when the workspace has no team.
pub(crate) fn store_team_flow_steps(
    paths: &GxserverPaths,
    workspace_id: &str,
    steps: Option<Value>,
) -> Option<Result<TeamFlowSteps, String>> {
    let connection = team_connection(paths, workspace_id)?;
    let mut args = Map::new();
    match steps {
        Some(steps) => args.insert("steps".to_string(), steps),
        None => args.insert("reset".to_string(), Value::Bool(true)),
    };
    Some(
        member_call(
            &connection,
            ConvexCallKind::Mutation,
            "teamFlowSteps:set",
            args,
        )
        .map(|answer| {
            let steps = parse(&answer);
            keep(workspace_id, &steps);
            steps
        }),
    )
}
