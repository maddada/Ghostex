//! `ghostex agent-hooks`: see, install and remove Ghostex's session resume hooks, on this
//! computer or (`--computer <name>`) on a connected one, which is how a computer that runs only
//! gxserver gets them. The work happens in gxserver (`agent_hooks::api`), the same calls Settings >
//! Agents makes.

use crate::ghostex_cli::{
    args::parse_args,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};
use serde_json::{json, Value};

const HELP: &str = "See, install, and remove Ghostex's session resume hooks for each agent CLI.

USAGE
  ghostex agent-hooks status [agent-id...] [--computer <name>] [--json]
  ghostex agent-hooks install <agent-id>... | --all [--computer <name>] [--json]
  ghostex agent-hooks uninstall <agent-id>... | --all [--computer <name>] [--json]

  The hooks let Ghostex resume an agent's sessions, show when it is working or
  done, and read its chat. status lists every agent Ghostex has a hook for:
  installed, missing, needs an update, or no CLI on that computer. install
  --all installs the hook of every agent CLI found there; uninstall --all
  removes every Ghostex hook and leaves your own hooks alone.

  --computer <name> runs it on a connected computer (ghostex computers), such
  as a Linux computer that runs only gxserver. Starting an agent there from
  another computer (coordinator start-thread --computer, agents create
  --computer) installs that agent's missing hook on its own.

EXAMPLES
  ghostex agent-hooks status
  ghostex agent-hooks install claude codex
  ghostex agent-hooks install --all --computer dell
";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    let json_output = flags.contains("json");
    let command = parsed.rest[0].as_str();
    let requested: Vec<String> = parsed.rest[1..]
        .iter()
        .map(|agent| agent.trim().to_ascii_lowercase())
        .filter(|agent| !agent.is_empty())
        .collect();
    let all = flags.truthy("all");
    let computer = flags
        .string_value("computer")
        .map(str::to_string)
        .unwrap_or_else(|| "this computer".to_string());
    let status = |params: Value| call_gxserver_rpc("/api/readAgentHookStatus", &params, flags);
    let endpoint = match command {
        "status" => None,
        "install" => Some("/api/installAgentHooks"),
        "uninstall" => Some("/api/uninstallAgentHooks"),
        other => {
            return Err(CliError::Other(format!(
                "Unknown agent-hooks command: {other}. Use status, install, or uninstall."
            )))
        }
    };
    // gxserver reads an unknown or empty agent list as every agent, so names are checked here.
    let catalog = status(json!({ "autoUpgradeInstalled": false }))?;
    let known: Vec<String> = rows(&catalog)
        .iter()
        .filter_map(|row| row["agentId"].as_str().map(str::to_string))
        .collect();
    if let Some(unknown) = requested.iter().find(|agent| !known.contains(agent)) {
        return Err(CliError::Other(format!(
            "Ghostex has no hook for {unknown}. Agents with hooks: {}.",
            known.join(", ")
        )));
    }
    let Some(endpoint) = endpoint else {
        let result = if requested.is_empty() {
            status(json!({}))?
        } else {
            status(json!({ "agentIds": requested }))?
        };
        if json_output {
            print_json(&result);
        } else {
            print_rows(&result, &requested);
        }
        return Ok(());
    };
    if requested.is_empty() && !all {
        return Err(CliError::Other(format!(
            "Name the agents ({command} claude codex) or pass --all."
        )));
    }
    if !requested.is_empty() && all {
        return Err(CliError::Other(
            "Name the agents or pass --all, not both.".to_string(),
        ));
    }
    let params = if all {
        json!({})
    } else {
        json!({ "agentIds": requested })
    };
    let result = call_gxserver_rpc(endpoint, &params, flags)?;
    if json_output {
        print_json(&result);
        return Ok(());
    }
    let shown: Vec<String> = if all { Vec::new() } else { requested.clone() };
    if command == "install" {
        let installed = agents_with(&result, &shown, |status| status == "installed");
        let not_installed = agents_with(&result, &shown, |status| status != "installed");
        if !installed.is_empty() {
            println!("Hooks installed on {computer}: {}.", installed.join(", "));
        }
        // With --all an agent whose CLI isn't there is expected; only named agents are reported.
        let failed: Vec<&String> = if all {
            Vec::new()
        } else {
            not_installed.iter().collect()
        };
        if !failed.is_empty() {
            print_rows_of(&result, &failed);
            return Err(CliError::Other(format!(
                "Some hooks were not installed on {computer}."
            )));
        }
        if installed.is_empty() {
            println!("No agent CLI with a Ghostex hook was found on {computer}.");
        }
    } else {
        let left = agents_with(&result, &shown, |status| {
            status == "installed" || status == "updateRequired"
        });
        if left.is_empty() {
            println!("Ghostex hooks removed on {computer}.");
        } else {
            println!(
                "Ghostex hooks removed on {computer}, except: {}.",
                left.join(", ")
            );
        }
    }
    Ok(())
}

fn rows(result: &Value) -> Vec<Value> {
    result["agents"].as_array().cloned().unwrap_or_default()
}

/// The agents (all of them when `shown` is empty) whose status passes `keep`.
fn agents_with(result: &Value, shown: &[String], keep: impl Fn(&str) -> bool) -> Vec<String> {
    rows(result)
        .iter()
        .filter(|row| {
            shown.is_empty()
                || row["agentId"]
                    .as_str()
                    .is_some_and(|agent| shown.iter().any(|wanted| wanted == agent))
        })
        .filter(|row| keep(row["status"].as_str().unwrap_or_default()))
        .filter_map(|row| row["agentId"].as_str().map(str::to_string))
        .collect()
}

fn print_rows(result: &Value, shown: &[String]) {
    let shown: Vec<&String> = shown.iter().collect();
    print_rows_of(result, &shown);
}

fn print_rows_of(result: &Value, shown: &[&String]) {
    println!("AGENT\tHOOK\tDETAIL");
    for row in rows(result) {
        let agent = row["agentId"].as_str().unwrap_or_default();
        if !shown.is_empty() && !shown.iter().any(|wanted| wanted.as_str() == agent) {
            continue;
        }
        let hook = match row["status"].as_str().unwrap_or_default() {
            "installed" => "installed",
            "missing" => "missing",
            "updateRequired" => "needs update",
            "cliMissing" => "no CLI",
            other => other,
        };
        println!(
            "{agent}\t{hook}\t{}",
            row["detail"].as_str().unwrap_or_default()
        );
    }
}
