//! `ghostex agents send --main-coordinator --project-id <id>|--path <folder> [--computer <name>]
//! [--if-none create|session] <text>`: a message to a project's main coordinator on a computer,
//! without knowing its session id.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User: the main coordinator "should be default one to contact in cli tool basically when any agent wants to reach out to another project basically or chief of staff wants to send something to be done in a thread in a project on a specific machine. (if there's no main coordinator on a [project] and user asked for main coordinator then create one, otherwise if user requested just some work to be done then it can be a regular thread to be created in that [project] to do this work and this thread gets communicated with by chief of staff or whoever launched it". With no main coordinator the caller picks one of two explicit choices, never a guess from its wording: `--if-none create` starts the project's coordinator (the agent and model `ghostex coordinator create` picks) with the message as its first request; `--if-none session` starts a regular session there with the message as its task, which reports to the caller (a coordinator's thread when the caller is a coordinator, else a session whose message says whom to reply to). Without `--if-none` it stops and names both.
//! SEE-ALSO: server/src/coordinators/main_coordinator.rs (which coordinator is main).

use serde_json::{json, Value};

use super::arguments::{Arguments, IfNone};
use super::identity::{self, text};
use crate::ghostex_cli::{
    args::parse_args,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The project the message is for: `--project-id`, or the project whose folder is `--path` on
/// that computer (`None` when no project has that folder yet).
fn project_of(args: &Arguments) -> CliResult<Option<String>> {
    if let Some(project_id) = &args.project_id {
        return Ok(Some(project_id.clone()));
    }
    let Some(path) = &args.path else {
        return Err(CliError::Other(
            "--main-coordinator needs the project: --project-id <id> or --path <folder> (ghostex sessions --json lists project ids; add --computer <name> for another computer's).".into(),
        ));
    };
    let wanted = path.trim_end_matches('/');
    let projects = call_gxserver_rpc("/api/listProjects", &json!({}), &args.flags)?;
    Ok(projects
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|project| {
            ["path", "projectPath", "cwd"]
                .iter()
                .any(|key| text(project, key).trim_end_matches('/') == wanted)
        })
        .map(|project| text(project, "projectId").to_string())
        .filter(|id| !id.is_empty()))
}

fn where_(args: &Arguments, project: &str) -> String {
    match args.flags.string_value("computer") {
        Some(computer) => format!("Project {project} on {computer}"),
        None => format!("Project {project}"),
    }
}

/// The project's main coordinator, from `/api/listCoordinators`; an error that lists the
/// coordinators when the computer's Ghostex is too old to mark one.
fn find_main(args: &Arguments, project_id: &str) -> CliResult<Option<Value>> {
    let listed = call_gxserver_rpc(
        "/api/listCoordinators",
        &json!({ "projectId": project_id }),
        &args.flags,
    )
    .map_err(|error| match args.flags.string_value("computer") {
        Some(computer) => CliError::Other(format!(
            "Could not read {computer}'s coordinators ({error}). If its Ghostex is older than this one, update it; until then pick a session there with ghostex sessions --computer {computer} and send with ghostex agents send <session-ref>."
        )),
        None => error,
    })?;
    let rows = listed["coordinators"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !rows.is_empty()
        && rows
            .iter()
            .all(|row| row.get("isMainCoordinator").is_none())
    {
        let names = rows
            .iter()
            .map(|row| format!("{} ({})", text(row, "title"), text(row, "globalRef")))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CliError::Other(format!(
            "That computer's Ghostex is older and does not mark a main coordinator; update it. Its coordinators in this project: {names}. Send to one with ghostex agents send <session-ref>."
        )));
    }
    Ok(rows
        .into_iter()
        .find(|row| row["isMainCoordinator"] == json!(true)))
}

fn message_body(args: &Arguments) -> CliResult<String> {
    let body = match &args.body_file {
        Some(path) => std::fs::read_to_string(path).map_err(|error| {
            CliError::Other(format!("Could not read message file {path}: {error}"))
        })?,
        None => args.positional.first().cloned().unwrap_or_default(),
    };
    if body.trim().is_empty() {
        return Err(CliError::Other("Message body must not be empty.".into()));
    }
    Ok(body)
}

/// A short title from the message's first words, for the session `--if-none session` starts.
fn title_from(body: &str) -> String {
    let words: Vec<&str> = body.split_whitespace().take(6).collect();
    let title = words.join(" ");
    title
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

pub(super) fn send(args: &Arguments) -> CliResult<Value> {
    let body = message_body(args)?;
    let project_id = project_of(args)?;
    let main = match &project_id {
        Some(project_id) => find_main(args, project_id)?,
        None => None,
    };
    let project_label = project_id
        .clone()
        .or_else(|| args.path.clone())
        .unwrap_or_default();
    if let Some(main) = main {
        let mut to_main = args.clone();
        to_main.positional = vec![text(&main, "globalRef").to_string(), body];
        to_main.body_file = None;
        let mut result = super::delivery::send(&to_main)?;
        result["mainCoordinator"] = json!({
            "globalRef": main["globalRef"],
            "title": main["title"],
            "projectId": main["projectId"],
        });
        return Ok(result);
    }
    match args.if_none {
        None => Err(CliError::Other(format!(
            "{} has no main coordinator. Choose what to do and run the command again with one of: --if-none create (start its main coordinator with the agent and model ghostex coordinator create picks, and send it this message) or --if-none session (start a regular session there to do this work; it reports back to you).",
            where_(args, &project_label)
        ))),
        Some(IfNone::Create) => {
            let caller = identity::caller().ok();
            let task = match &caller {
                Some(caller) => identity::message(caller, &body),
                None => body.clone(),
            };
            let mut argv = vec!["--task".to_string(), task];
            match &project_id {
                Some(project_id) => argv.extend(["--project-id".to_string(), project_id.clone()]),
                None => argv.extend([
                    "--path".to_string(),
                    args.path.clone().unwrap_or_default(),
                ]),
            }
            for key in ["computer", "server"] {
                if let Some(value) = args.flags.string_value(key) {
                    argv.extend([format!("--{key}"), value.to_string()]);
                }
            }
            if let Some(title) = &args.title {
                argv.extend(["--title".to_string(), title.clone()]);
            }
            let mut result =
                crate::ghostex_cli::coordinator::create_coordinator(&parse_args(&argv))?;
            result["mainCoordinator"] = json!({
                "globalRef": result["globalRef"],
                "title": result["title"],
                "created": true,
            });
            result["note"] = json!(format!(
                "{} had no main coordinator: this new coordinator is its main one now, and your message is its first request.",
                where_(args, &project_label)
            ));
            Ok(result)
        }
        Some(IfNone::Session) => {
            let caller = identity::caller().map_err(|error| {
                CliError::Other(format!(
                    "{error} --if-none session starts a session that reports to you, so run it from an agent session."
                ))
            })?;
            let mut create = args.clone();
            create.command = "create".to_string();
            create.positional = vec![text(&caller, "agentId").to_string()];
            create.task = Some(body.clone());
            create.body_file = None;
            create.project_id = project_id.clone();
            create.title = Some(args.title.clone().unwrap_or_else(|| title_from(&body)));
            let mut result = super::lifecycle::create(&create)?;
            result["note"] = json!(format!(
                "{} has no main coordinator, so a regular session there does this work; it reports back to you.",
                where_(args, &project_label)
            ));
            Ok(result)
        }
    }
}
