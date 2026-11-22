//! A cloud session started from the Work page or `ghostex work-mode start --cloud`
//! (work_mode/cloud_work.rs), added to the team's `ticketSessions` the way a Slack request's
//! session is (`packages/team-sync/convex/workCloudSessions.ts`), so teammates see it on the ticket
//! and a later Slack request for the ticket goes to it.

use serde_json::{json, Map};

use crate::paths::GxserverPaths;

use super::convex_http::ConvexCallKind;
use super::operations::{connection_for, member_call};
use super::work_page::{forget_team_ticket, team_error};

pub(crate) struct TeamCloudSession<'a> {
    /// The team's ticket key (`SPX-1245`, `owner/repo#12`).
    pub(crate) ticket: &'a str,
    pub(crate) session_url: &'a str,
    pub(crate) branch: &'a str,
    pub(crate) project_id: &'a str,
    pub(crate) runner_name: &'a str,
}

/// Blocking: one Convex mutation with this computer's member token.
pub(crate) fn record_team_cloud_session(
    paths: &GxserverPaths,
    workspace_id: &str,
    session: &TeamCloudSession<'_>,
) -> Result<(), String> {
    let connection = connection_for(paths, workspace_id)?;
    let mut args = Map::new();
    args.insert("ticket".to_string(), json!(session.ticket));
    args.insert("sessionUrl".to_string(), json!(session.session_url));
    args.insert("branch".to_string(), json!(session.branch));
    args.insert("projectId".to_string(), json!(session.project_id));
    args.insert("runnerName".to_string(), json!(session.runner_name));
    member_call(
        &connection,
        ConvexCallKind::Mutation,
        "workCloudSessions:record",
        args,
    )
    .map_err(|error| team_error(&connection, error))?;
    forget_team_ticket(workspace_id, session.ticket);
    Ok(())
}
