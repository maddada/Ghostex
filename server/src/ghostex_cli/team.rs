//! `ghostex team`: a Work workspace's connection to its team's own Convex project
//! (crate::team_sync). Every verb takes `--workspace <name or id>`; without it, the only Work
//! workspace is used.

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::args::{parse_args, Flags};
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};
use crate::paths::get_gxserver_paths;
use crate::team_sync::{deploy_team_functions, site_url, DeployOptions};

const PING_WAIT: Duration = Duration::from_secs(20);

/// `ghostex team join|status|deploy|invite|identity|threads|ping|leave|flow|slack-manifest|slack-connect|linear-connect|own-linear-key …`.
pub(super) fn team_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("status");
    let positional = parsed.rest.get(1).cloned();
    match subcommand {
        "join" => {
            let link = positional.ok_or_else(|| {
                CliError::Other("Pass the invite link: ghostex team join <link>.".to_string())
            })?;
            let mut params = workspace_params(flags)?;
            params.insert("inviteLink".to_string(), json!(link));
            params.insert(
                "name".to_string(),
                json!(flags
                    .string_value("name")
                    .map(str::to_string)
                    .unwrap_or_else(default_member_name)),
            );
            if let Some(slack_user) = flags.string_value("slackUser") {
                params.insert("slackUserId".to_string(), json!(slack_user));
            }
            if let Some(code) = flags.string_value("slackCode") {
                params.insert("slackLinkCode".to_string(), json!(code));
            }
            call_and_print("/api/joinTeamSync", params, flags)
        }
        "status" => {
            let mut params = Map::new();
            if flags.string_value("workspace").is_some() || flags.string_value("workspaceId").is_some()
            {
                params = workspace_params(flags)?;
            }
            call_and_print("/api/readTeamSyncStatus", params, flags)
        }
        "invite" => call_and_print("/api/createTeamSyncInvite", workspace_params(flags)?, flags),
        "identity" => {
            let mut params = workspace_params(flags)?;
            for (flag, key) in [
                ("slackUser", "slackUserId"),
                ("slackCode", "slackLinkCode"),
                ("linearUser", "linearUserId"),
                ("name", "name"),
            ] {
                if let Some(value) = flags.string_value(flag) {
                    params.insert(key.to_string(), json!(value));
                }
            }
            call_and_print("/api/setTeamSyncIdentity", params, flags)
        }
        "threads" => {
            let ticket = positional.ok_or_else(|| {
                CliError::Other("Pass the ticket: ghostex team threads SPX-1234.".to_string())
            })?;
            let mut params = workspace_params(flags)?;
            params.insert("ticket".to_string(), json!(ticket));
            call_and_print("/api/listTicketSlackThreads", params, flags)
        }
        "ping" => ping(flags),
        "leave" => call_and_print("/api/leaveTeamSync", workspace_params(flags)?, flags),
        "deploy" => deploy(flags),
        "flow" => super::team_slack::flow_command(&parsed.rest[1..], flags),
        "slack-manifest" => super::team_slack::slack_manifest_command(flags),
        "slack-connect" => super::team_slack::slack_connect_command(flags),
        "linear-connect" => super::team_slack::linear_connect_command(flags),
        "own-linear-key" => super::team_slack::own_linear_key_command(&parsed.rest[1..], flags),
        other => Err(CliError::Other(format!(
            "Unknown team command \"{other}\". Use join, status, deploy, invite, identity, threads, ping, leave, flow, slack-manifest, slack-connect, linear-connect or own-linear-key."
        ))),
    }
}

pub(super) fn call_and_print(
    path: &str,
    params: Map<String, Value>,
    flags: &Flags,
) -> CliResult<()> {
    let result = rpc::call_gxserver_rpc(path, &Value::Object(params), flags)?;
    print_json(&result);
    Ok(())
}

fn default_member_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Teammate".to_string())
}

/// `{ workspaceId }` for `--workspace <name or id>`, or the only Work workspace.
pub(super) fn workspace_params(flags: &Flags) -> CliResult<Map<String, Value>> {
    let mut params = Map::new();
    params.insert(
        "workspaceId".to_string(),
        json!(resolve_workspace_id(flags)?),
    );
    Ok(params)
}

pub(super) fn resolve_workspace_id(flags: &Flags) -> CliResult<String> {
    let wanted = flags
        .string_value("workspace")
        .or_else(|| flags.string_value("workspaceId"))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let workspaces = rpc::call_gxserver_rpc("/api/readWorkspaces", &json!({}), flags)
        .ok()
        .and_then(|result| {
            result
                .pointer("/sidebarWorkspaces/workspaces")
                .and_then(Value::as_object)
                .cloned()
        })
        .unwrap_or_default();
    let name_of = |workspace: &Value| {
        workspace
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    if let Some(wanted) = wanted {
        if workspaces.is_empty() || workspaces.contains_key(wanted) {
            return Ok(wanted.to_string());
        }
        return workspaces
            .iter()
            .find(|(_, workspace)| name_of(workspace).eq_ignore_ascii_case(wanted))
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                CliError::Other(format!(
                    "No workspace named \"{wanted}\". Workspaces: {}.",
                    workspaces
                        .values()
                        .map(name_of)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            });
    }
    let work: Vec<&String> = workspaces
        .iter()
        .filter(|(_, workspace)| workspace.get("kind").and_then(Value::as_str) == Some("work"))
        .map(|(id, _)| id)
        .collect();
    match work.as_slice() {
        [only] => Ok((*only).clone()),
        _ => Err(CliError::Other(format!(
            "Pass --workspace <name>. Workspaces: {}.",
            if workspaces.is_empty() {
                "none yet".to_string()
            } else {
                workspaces
                    .values()
                    .map(name_of)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ))),
    }
}

/// Queues a `ping` for this member and waits for this computer's Ghostex to answer it through the
/// live subscription.
fn ping(flags: &Flags) -> CliResult<()> {
    let params = workspace_params(flags)?;
    let queued =
        rpc::call_gxserver_rpc("/api/pingTeamSync", &Value::Object(params.clone()), flags)?;
    let command_id = queued
        .get("commandId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let started = Instant::now();
    while started.elapsed() < PING_WAIT {
        std::thread::sleep(Duration::from_millis(500));
        let recent = rpc::call_gxserver_rpc(
            "/api/listTeamSyncCommands",
            &Value::Object(params.clone()),
            flags,
        )?;
        let command = recent
            .get("commands")
            .and_then(Value::as_array)
            .and_then(|commands| {
                commands
                    .iter()
                    .find(|command| command.get("id").and_then(Value::as_str) == Some(&command_id))
            })
            .cloned();
        if let Some(command) = command {
            if matches!(
                command.get("status").and_then(Value::as_str),
                Some("done" | "failed")
            ) {
                print_json(&json!({
                    "ok": true,
                    "commandId": command_id,
                    "roundTripMs": started.elapsed().as_millis() as u64,
                    "command": command,
                }));
                return Ok(());
            }
        }
    }
    Err(CliError::Other(format!(
        "Queued ping {command_id}, but this computer's Ghostex did not answer within {}s. Check `ghostex team status`.",
        PING_WAIT.as_secs()
    )))
}

/// Deploys the functions with the person's Convex CLI login, creates the team on the first
/// deploy, and connects this workspace as its owner.
fn deploy(flags: &Flags) -> CliResult<()> {
    let workspace_id = resolve_workspace_id(flags)?;
    let options = DeployOptions {
        workspace_id: workspace_id.clone(),
        project: flags.string_value("project").map(str::to_string),
        convex_team: flags.string_value("convexTeam").map(str::to_string),
        team_name: flags.string_value("teamName").map(str::to_string),
        owner_name: flags.string_value("name").map(str::to_string),
        dev: flags.truthy("dev"),
    };
    let outcome =
        deploy_team_functions(&get_gxserver_paths(None), &options).map_err(CliError::Other)?;
    let connection = match outcome.owner_member_token.as_deref() {
        Some(member_token) => Some(rpc::call_gxserver_rpc(
            "/api/connectTeamSync",
            &json!({
                "workspaceId": workspace_id,
                "deploymentUrl": outcome.deployment_url,
                "memberToken": member_token,
            }),
            flags,
        )?),
        None => None,
    };
    let site = site_url(&outcome.deployment_url);
    print_json(&json!({
        "ok": true,
        "workspaceId": workspace_id,
        "deploymentUrl": outcome.deployment_url,
        "functionsDir": outcome.functions_dir.to_string_lossy(),
        "createdTeam": connection.is_some(),
        "connection": connection,
        "slackEventsUrl": site.as_ref().map(|site| format!("{site}/slack/events")),
        "slackCommandsUrl": site.as_ref().map(|site| format!("{site}/slack/commands")),
        "slackInteractivityUrl": site.as_ref().map(|site| format!("{site}/slack/interactivity")),
        "linearWebhookUrl": site.as_ref().map(|site| format!("{site}/linear/webhook")),
        "next": if connection.is_some() {
            "Create the Slack app from `ghostex team slack-manifest`, store its secrets with `ghostex team slack-connect` and a Linear key with `ghostex team linear-connect`, set the working channel with `ghostex team flow set --working-channel <ID>`, then share `ghostex team invite`."
        } else {
            "The functions were updated. This deployment already had a team: join it with an invite link (ghostex team join) if this workspace is not connected yet."
        },
    }));
    Ok(())
}
