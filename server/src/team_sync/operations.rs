//! The blocking operations behind the team-sync endpoints: join, connect, invite, identity,
//! status, a ticket's Slack threads, ping and leave. Each takes the workspace id the connection is
//! keyed by.

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

use super::connections::{
    read_team_connection, read_team_connections, remove_team_connection, store_team_connection,
    TeamConnection,
};
use super::convex_http::{call_convex, ConvexCallKind};
use super::invite_link::{build_invite_link, normalize_deployment_url, parse_invite_link};
use super::runtime::{reload_team_sync, subscription_status};

pub(super) fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn required(params: &Map<String, Value>, key: &str) -> Result<String, String> {
    text(params, key).ok_or_else(|| format!("Pass {key}."))
}

pub(super) fn connection_for(
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<TeamConnection, String> {
    read_team_connection(paths, workspace_id).ok_or_else(|| {
        format!(
            "Workspace \"{workspace_id}\" is not connected to a team. Join with an invite link (ghostex team join) or set one up (ghostex team deploy)."
        )
    })
}

fn save_connection(paths: &GxserverPaths, connection: &TeamConnection) -> Result<(), String> {
    store_team_connection(paths, connection)
        .map_err(|error| format!("Could not save the team connection: {error}"))?;
    reload_team_sync();
    Ok(())
}

pub(super) fn member_call(
    connection: &TeamConnection,
    kind: ConvexCallKind,
    path: &str,
    mut args: Map<String, Value>,
) -> Result<Value, String> {
    args.insert("memberToken".to_string(), json!(connection.member_token));
    call_convex(&connection.deployment_url, kind, path, Value::Object(args))
}

fn default_member_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Teammate".to_string())
}

/// `{ workspaceId, inviteLink, name?, slackUserId?, slackLinkCode? }`: exchanges the invite's
/// one-time code for a member token and connects the workspace. A Slack user is linked only with the
/// one-time code Ghostex sent that user in Slack; a link Convex refuses leaves the join in place and
/// is reported as `slackLinkError`.
pub(crate) fn join_team(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let workspace_id = required(params, "workspaceId")?;
    let (deployment_url, code) = parse_invite_link(&required(params, "inviteLink")?)?;
    // Settings sends no name; the teammate list then shows this computer's user name, as the CLI does.
    let name = text(params, "name").unwrap_or_else(default_member_name);
    let joined = call_convex(
        &deployment_url,
        ConvexCallKind::Action,
        "invites:join",
        json!({ "code": code, "name": name }),
    )?;
    let member_token = joined
        .get("memberToken")
        .and_then(Value::as_str)
        .ok_or("The team's Convex project did not return a member token.")?
        .to_string();
    let connection = TeamConnection {
        workspace_id,
        deployment_url,
        member_token,
        team_name: joined
            .get("teamName")
            .and_then(Value::as_str)
            .map(str::to_string),
        member_id: joined
            .get("memberId")
            .and_then(Value::as_str)
            .map(str::to_string),
        member_name: Some(name),
        connected_at: Some(chrono::Utc::now().to_rfc3339()),
        own_linear_key: false,
    };
    save_connection(paths, &connection)?;
    let mut summary = connection.summary();
    if let Some(slack_user_id) = text(params, "slackUserId") {
        let mut args = Map::new();
        args.insert("slackUserId".to_string(), json!(slack_user_id));
        if let Some(code) = text(params, "slackLinkCode") {
            args.insert("slackLinkCode".to_string(), json!(code));
        }
        if let Err(error) = member_call(
            &connection,
            ConvexCallKind::Mutation,
            "teams:setIdentity",
            args,
        ) {
            if let Some(object) = summary.as_object_mut() {
                object.insert("slackLinkError".to_string(), json!(error));
            }
        }
    }
    Ok(summary)
}

/// `{ workspaceId, deploymentUrl, memberToken }`: connects a workspace with a token made elsewhere
/// (`ghostex team deploy` makes the owner's). The token is checked before it is saved.
pub(crate) fn connect_team(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let workspace_id = required(params, "workspaceId")?;
    let deployment_url = normalize_deployment_url(&required(params, "deploymentUrl")?)?;
    let member_token = required(params, "memberToken")?;
    let mut connection = TeamConnection {
        workspace_id,
        deployment_url,
        member_token,
        team_name: None,
        member_id: None,
        member_name: None,
        connected_at: Some(chrono::Utc::now().to_rfc3339()),
        own_linear_key: false,
    };
    let info = member_call(&connection, ConvexCallKind::Query, "teams:info", Map::new())?;
    connection.team_name = info
        .pointer("/team/name")
        .and_then(Value::as_str)
        .map(str::to_string);
    connection.member_id = info
        .pointer("/me/id")
        .and_then(Value::as_str)
        .map(str::to_string);
    connection.member_name = info
        .pointer("/me/name")
        .and_then(Value::as_str)
        .map(str::to_string);
    save_connection(paths, &connection)?;
    Ok(connection.summary())
}

/// `{ workspaceId? }`: every connection (or one), with the live subscription state and, when
/// `live` is not false, the team as Convex sees it now.
pub(crate) fn team_status(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let only = text(params, "workspaceId");
    let live = params.get("live").and_then(Value::as_bool).unwrap_or(true);
    let connections: Vec<Value> = read_team_connections(paths)
        .into_iter()
        .filter(|connection| {
            only.as_deref()
                .is_none_or(|id| id == connection.workspace_id)
        })
        .map(|connection| {
            let mut summary = connection.summary();
            summary["subscription"] = json!(subscription_status(&connection.workspace_id));
            if live {
                match member_call(&connection, ConvexCallKind::Query, "teams:info", Map::new()) {
                    Ok(info) => summary["team"] = info,
                    Err(error) => summary["teamError"] = json!(error),
                }
            }
            summary
        })
        .collect();
    Ok(json!({ "connections": connections }))
}

/// `{ workspaceId }`: a new one-time invite link for this workspace's team.
pub(crate) fn create_invite(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    let invite = member_call(
        &connection,
        ConvexCallKind::Action,
        "invites:create",
        Map::new(),
    )?;
    let code = invite
        .get("code")
        .and_then(Value::as_str)
        .ok_or("The team's Convex project did not return an invite code.")?;
    Ok(json!({
        "inviteLink": build_invite_link(&connection.deployment_url, code),
        "expiresAt": invite.get("expiresAt").cloned().unwrap_or(Value::Null),
    }))
}

/// `{ workspaceId, slackUserId?, slackLinkCode?, linearUserId?, name? }` (an empty string clears an
/// id). Linking a Slack user needs the one-time code Ghostex sent that user privately in Slack.
pub(crate) fn set_identity(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    let mut args = Map::new();
    for key in ["slackUserId", "linearUserId"] {
        if let Some(value) = params.get(key).and_then(Value::as_str) {
            let value = value.trim();
            args.insert(
                key.to_string(),
                if value.is_empty() {
                    Value::Null
                } else {
                    json!(value)
                },
            );
        }
    }
    if let Some(name) = text(params, "name") {
        args.insert("name".to_string(), json!(name));
    }
    if args.is_empty() {
        return Err("Pass slackUserId, linearUserId or name.".to_string());
    }
    if let Some(code) = text(params, "slackLinkCode") {
        args.insert("slackLinkCode".to_string(), json!(code));
    }
    member_call(
        &connection,
        ConvexCallKind::Mutation,
        "teams:setIdentity",
        args,
    )
}

/// `{ workspaceId, ticket }`: the ticket's Slack threads (working thread first) with their messages.
pub(crate) fn list_ticket_slack_threads(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let workspace_id = required(params, "workspaceId")?;
    let ticket = required(params, "ticket")?;
    let Some(connection) = read_team_connection(paths, &workspace_id) else {
        // A workspace with no team simply has no Slack threads; the Work page shows none.
        return Ok(
            json!({ "workspaceId": workspace_id, "ticket": ticket, "connected": false, "threads": [] }),
        );
    };
    let mut args = Map::new();
    args.insert("ticket".to_string(), json!(ticket));
    let threads = member_call(
        &connection,
        ConvexCallKind::Query,
        "slackThreads:listForTicket",
        args,
    )?;
    Ok(
        json!({ "workspaceId": workspace_id, "ticket": ticket, "connected": true, "threads": threads }),
    )
}

/// `{ workspaceId }`: queues a `ping` for this member; the subscription answers it.
pub(crate) fn ping_team(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    let mut args = Map::new();
    args.insert("type".to_string(), json!("ping"));
    let command_id = member_call(
        &connection,
        ConvexCallKind::Mutation,
        "commands:enqueue",
        args,
    )?;
    Ok(json!({ "commandId": command_id }))
}

/// `{ workspaceId, limit? }`: this member's recent commands, newest first.
pub(crate) fn recent_commands(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    let mut args = Map::new();
    if let Some(limit) = params.get("limit").and_then(Value::as_u64) {
        args.insert("limit".to_string(), json!(limit));
    }
    let commands = member_call(
        &connection,
        ConvexCallKind::Query,
        "commands:listRecent",
        args,
    )?;
    Ok(json!({ "commands": commands }))
}

/// `{ workspaceId }`: leaves the team (the token stops working) and forgets the connection. A team
/// that cannot be reached is still forgotten here.
pub(crate) fn leave_team(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let workspace_id = required(params, "workspaceId")?;
    let connection = connection_for(paths, &workspace_id)?;
    let remote_error = member_call(
        &connection,
        ConvexCallKind::Mutation,
        "invites:leave",
        Map::new(),
    )
    .err();
    remove_team_connection(paths, &workspace_id)
        .map_err(|error| format!("Could not forget the team connection: {error}"))?;
    reload_team_sync();
    Ok(
        json!({ "workspaceId": workspace_id, "left": remote_error.is_none(), "remoteError": remote_error }),
    )
}
