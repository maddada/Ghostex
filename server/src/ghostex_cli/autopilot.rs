//! `ghostex autopilot`: turn Autopilot on and off, read what it did, and take a project or a
//! session out of it (`/api/autopilot`, server/src/server/autopilot_http.rs).

use serde_json::{json, Value};

use super::args::Flags;
use super::rpc::{call_gxserver_rpc, CliError, CliResult};

const HELP: &str = "Usage: ghostex autopilot [status|on|off|log|project|session] [--json]

Autopilot keeps sessions going while you are away: it carries on a session that stopped while its
background job finished or mid-task, interrupts and carries on one stuck on a hung command, reloads
one that stays frozen, moves one at a usage limit to an account with room, and restarts one whose
terminal ended. Every decision is logged. Settings > Autopilot has every choice.

  ghostex autopilot                        Whether it is on, what it covers, and what it did
  ghostex autopilot on | off               Turn it on or off for everything it covers
  ghostex autopilot log [--last <n>]       Its newest decisions (20 by default)
  ghostex autopilot project <project-id> on|off
                                           Put a project (and its worktrees) in or out
  ghostex autopilot session <project-id> <session-id> on|off
                                           Put one session in or out
  ghostex autopilot decide <project-id> <session-id> --answer <pick> [--answer ...]
                         | --message <text> | --leave <note>
                                           The supervisor's decision for one session: answer
                                           its open questions (one --answer per question, in
                                           order), tell it how to carry on, or leave it for you";

fn call(params: Value) -> CliResult<Value> {
    call_gxserver_rpc("/api/autopilot", &params, &Flags::default())
}

fn on_off(word: Option<&String>) -> CliResult<bool> {
    match word.map(String::as_str) {
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        _ => Err(CliError::Other("Say on or off.".to_string())),
    }
}

pub(crate) fn autopilot_command(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{HELP}");
        return Ok(());
    }
    let json_out = args.iter().any(|arg| arg == "--json");
    if args.first().map(String::as_str) == Some("decide") {
        return decide(args, json_out);
    }
    let words: Vec<&String> = args.iter().filter(|arg| !arg.starts_with("--")).collect();
    let last = args
        .iter()
        .position(|arg| arg == "--last")
        .and_then(|at| args.get(at + 1))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(20);
    let words: Vec<&String> = words
        .into_iter()
        .filter(|word| word.parse::<u64>().map_or(true, |n| n != last))
        .collect();
    let answer = match words.first().map(|word| word.as_str()) {
        None | Some("status") => call(json!({ "op": "read", "limit": 5 }))?,
        Some("on") | Some("off") => call(json!({
            "op": "set",
            "patch": { "enabled": words[0] == "on" },
            "limit": 5,
        }))?,
        Some("log") => call(json!({ "op": "read", "limit": last }))?,
        Some("project") => {
            let project_id = words
                .get(1)
                .ok_or_else(|| CliError::Other("Name the project id.".to_string()))?;
            call(json!({
                "op": "setProject",
                "projectId": project_id,
                "on": on_off(words.get(2).copied())?,
                "limit": 5,
            }))?
        }
        Some("session") => {
            let (Some(project_id), Some(session_id)) = (words.get(1), words.get(2)) else {
                return Err(CliError::Other(
                    "Name the project id and the session id.".to_string(),
                ));
            };
            call(json!({
                "op": "setSession",
                "projectId": project_id,
                "sessionId": session_id,
                "on": on_off(words.get(3).copied())?,
                "limit": 5,
            }))?
        }
        Some(other) => {
            return Err(CliError::Other(format!(
                "Unknown autopilot command \"{other}\".\n\n{HELP}"
            )))
        }
    };
    if json_out {
        super::output::print_json(&answer);
        return Ok(());
    }
    let config = &answer["config"];
    let on = config["enabled"].as_bool() == Some(true);
    if words.first().map(|word| word.as_str()) != Some("log") {
        println!(
            "Autopilot is {}{}.",
            if on { "on" } else { "off" },
            if on {
                format!(
                    " ({}, sweeping every {} seconds, {} sessions covered)",
                    match config["scope"].as_str() {
                        Some("projects") => "projects you picked",
                        Some("sessions") => "sessions you picked",
                        _ => "every session",
                    },
                    config["sweepSeconds"],
                    answer["watching"]["covered"]
                )
            } else {
                String::new()
            }
        );
        let summary = &answer["summary"];
        println!(
            "Since it went on: {} actions, {} sessions kept moving, {} left for you, {} failed.",
            summary["actions"], summary["keptMoving"], summary["parkedCount"], summary["failed"]
        );
    }
    for entry in answer["log"].as_array().into_iter().flatten() {
        println!(
            "{}  {} ({})\n    Saw: {}\n    Did: {}",
            entry["at"].as_str().unwrap_or_default(),
            entry["title"].as_str().unwrap_or_default(),
            entry["project"].as_str().unwrap_or_default(),
            entry["saw"].as_str().unwrap_or_default(),
            entry["did"].as_str().unwrap_or_default(),
        );
    }
    Ok(())
}

/// `ghostex autopilot decide <project-id> <session-id> --answer … | --message … | --leave …`.
fn decide(args: &[String], json_out: bool) -> CliResult<()> {
    let mut positional = Vec::new();
    let mut answers = Vec::new();
    let mut message = None;
    let mut leave = None;
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        let value = |rest: &mut std::iter::Skip<std::slice::Iter<'_, String>>| {
            rest.next()
                .cloned()
                .ok_or_else(|| CliError::Other(format!("{arg} needs a value.")))
        };
        match arg.as_str() {
            "--answer" => answers.push(value(&mut rest)?),
            "--message" => message = Some(value(&mut rest)?),
            "--leave" => leave = Some(value(&mut rest)?),
            "--json" => {}
            other => positional.push(other.to_string()),
        }
    }
    let [project_id, session_id] = positional.as_slice() else {
        return Err(CliError::Other(format!(
            "Name the project id and the session id.\n\n{HELP}"
        )));
    };
    let answer = call(json!({
        "op": "decide",
        "projectId": project_id,
        "sessionId": session_id,
        "answers": answers,
        "message": message,
        "leave": leave,
    }))?;
    if json_out {
        super::output::print_json(&answer);
    } else {
        println!("{}", answer["did"].as_str().unwrap_or("Done."));
    }
    Ok(())
}
