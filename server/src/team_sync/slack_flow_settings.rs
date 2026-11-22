//! The team flow settings the Slack command flow reads (working channel, watch-only channels,
//! channel → repo mapping, default run place, team instructions), the Slack app manifest, and the
//! Slack secrets the team's Convex deployment needs (`ghostex team slack-connect`).

use std::io::Write;
use std::process::Stdio;

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

use super::convex_http::ConvexCallKind;
use super::deploy::{convex_cli, ensure_dependencies, functions_dir};
use super::invite_link::site_url;
use super::operations::{connection_for, member_call, required};

/// The fields `teamFlow:set` accepts, passed through as given.
const TEAM_FLOW_FIELDS: &[&str] = &[
    "workingChannelId",
    "watchOnlyChannelIds",
    "channelRepos",
    "mapChannel",
    "unmapChannel",
    "linearTeamKey",
    "defaultRunPlace",
    "qcOwnerSlackUserId",
    "instructions",
    // `linear` or `github`: the workspace's primary tracker (crate::work_mode::tracker).
    "tracker",
];

/// Whether this computer's member of the workspace is connected to a team.
pub(crate) fn workspace_has_team(paths: &GxserverPaths, workspace_id: &str) -> bool {
    super::connections::read_team_connection(paths, workspace_id).is_some()
}

/// `{ workspaceId }`: the team's flow settings.
pub(crate) fn read_team_flow(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    member_call(
        &connection,
        ConvexCallKind::Query,
        "teamFlow:get",
        Map::new(),
    )
}

/// `{ workspaceId, …teamFlow:set fields }`: changes the team's flow settings and returns them.
pub(crate) fn set_team_flow(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    let args: Map<String, Value> = TEAM_FLOW_FIELDS
        .iter()
        .filter_map(|key| {
            params
                .get(*key)
                .map(|value| (key.to_string(), value.clone()))
        })
        .collect();
    if args.is_empty() {
        return Err(format!("Pass one of: {}.", TEAM_FLOW_FIELDS.join(", ")));
    }
    member_call(&connection, ConvexCallKind::Mutation, "teamFlow:set", args)
}

/// `{ workspaceId, name? }`: the Slack app manifest for this team, with every request URL on the
/// team's Convex deployment.
///
/// CDXC:TeamSync 2026-10-09 DECISION:
/// User: one Slack app per team, created from a manifest Ghostex gives.
pub(crate) fn slack_manifest(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, String> {
    let connection = connection_for(paths, &required(params, "workspaceId")?)?;
    let site = site_url(&connection.deployment_url).ok_or(
        "This team's Convex deployment is not on convex.cloud, so its HTTP actions URL is unknown.",
    )?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Ghostex");
    let manifest = json!({
        "display_information": {
            "name": name,
            "description": "Starts and follows Ghostex work from Slack threads.",
            "background_color": "#161616"
        },
        "features": {
            "bot_user": { "display_name": name, "always_online": true },
            "slash_commands": [{
                "command": "/ghostex",
                "url": format!("{site}/slack/commands"),
                "description": "Start work from the top of a channel",
                "usage_hint": "cloud|local <what to do>",
                "should_escape": false
            }]
        },
        "oauth_config": {
            "scopes": {
                "bot": [
                    "app_mentions:read",
                    "channels:history",
                    "groups:history",
                    // Channel names for the Work page's Slack threads (conversations.info).
                    "channels:read",
                    "groups:read",
                    "chat:write",
                    "chat:write.customize",
                    "reactions:write",
                    "users:read",
                    "commands",
                    "files:read"
                ]
            }
        },
        "settings": {
            "event_subscriptions": {
                "request_url": format!("{site}/slack/events"),
                "bot_events": ["app_mention", "message.channels", "message.groups"]
            },
            "interactivity": {
                "is_enabled": true,
                "request_url": format!("{site}/slack/interactivity")
            },
            "org_deploy_enabled": false,
            "socket_mode_enabled": false,
            "token_rotation_enabled": false
        }
    });
    Ok(json!({
        "manifest": manifest,
        "next": "Create the app at https://api.slack.com/apps → Create New App → From a manifest, install it to your workspace, then run `ghostex team slack-connect` and paste the Bot User OAuth Token and the Signing Secret. Then open Event Subscriptions and let Slack verify the request URL again."
    }))
}

/// Sets environment variables on the team's Convex deployment with the Convex CLI login of the
/// person who deployed it, piping each value through stdin so it never shows in a process list.
/// Runs in the `ghostex` CLI process (it needs the deploy folder and that login).
pub(crate) fn set_deployment_env(
    paths: &GxserverPaths,
    workspace_id: &str,
    dev: bool,
    vars: &[(&str, &str)],
) -> Result<(), String> {
    let dir = functions_dir(paths, workspace_id);
    if !dir.join(".env.local").is_file() {
        return Err(format!(
            "This computer did not deploy the team's functions for \"{workspace_id}\". Run this where `ghostex team deploy` ran (the Convex project's admin)."
        ));
    }
    ensure_dependencies(&dir)?;
    for (name, value) in vars {
        let mut args = vec!["env", "set", name];
        if !dev {
            args.push("--prod");
        }
        let mut child = convex_cli(&dir, &args)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| {
                format!("Could not run the Convex CLI ({error}). Is Node.js installed?")
            })?;
        child
            .stdin
            .take()
            .ok_or("Could not write to the Convex CLI.")?
            .write_all(value.as_bytes())
            .map_err(|error| format!("Could not write to the Convex CLI: {error}"))?;
        let status = child
            .wait()
            .map_err(|error| format!("The Convex CLI failed: {error}"))?;
        if !status.success() {
            return Err(format!(
                "`convex env set {name}` failed; see the output above."
            ));
        }
    }
    Ok(())
}
