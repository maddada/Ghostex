//! `ghostex chief`: Chief, the Ghostex-wide coordinator (server/src/chief/).
//!
//! - `status [--computer <name>] [--json]`: whether Chief is on, its session and its agent.
//! - `guide`: the playbook Chief follows.
//! - `open [--agent <id>]`, `new-chat`, `set-agent <id>`: the user's own terminal only; a session's
//!   token is refused (gxserver checks it).
//! - `send [--computer <name>] "<text>"`: a message to this computer's Chief, or another
//!   computer's, the way `ghostex agents send` delivers it.

use serde_json::{json, Value};

use super::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const HELP: &str = "Main Hub: the Ghostex-wide coordinator pinned under the Spaces bar.

USAGE
  ghostex chief status [--computer <name>] [--json]
  ghostex chief guide
  ghostex chief open [--agent <agent-id>] [--json]
  ghostex chief new-chat [--json]
  ghostex chief set-agent <agent-id> [--json]
  ghostex chief send [--computer <name>] \"<text>\"

status says whether Main Hub is on, which session it is and which agent runs it.
guide prints the playbook Main Hub follows. open starts Main Hub on the agent you
pick (the first time only); new-chat starts it fresh, keeping its memory and
threads; set-agent moves it to another agent the same way. Only you can run
those three, from your own terminal or Main Hub's menu in the sidebar. send
messages Main Hub (another computer's with --computer), which is how one Main Hub
hands work to the Main Hub of the computer it runs on.
";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| arg == "-h" || arg == "--help" || arg == "help")
    {
        print!("{HELP}");
        return Ok(());
    }
    let parsed = parse_args(&args[1..]);
    let flags = target_flags(&parsed.flags);
    let json_output = parsed.flags.truthy("json");
    match args[0].as_str() {
        "status" => {
            let status = call_chief("status", json!({}), &flags)?;
            if json_output {
                print_json(&status);
            } else {
                print_status(&status);
            }
            Ok(())
        }
        "guide" => {
            let guide = call_chief("guide", json!({}), &flags)?;
            print!("{}", guide["guide"].as_str().unwrap_or_default());
            Ok(())
        }
        "open" => change(
            "open",
            json!({ "agentId": parsed.flags.text("agent") }),
            &flags,
            json_output,
        ),
        "new-chat" => change("newChat", json!({}), &flags, json_output),
        "set-agent" => {
            let agent = parsed
                .rest
                .first()
                .cloned()
                .or_else(|| parsed.flags.text("agent"))
                .ok_or_else(|| {
                    CliError::Other(
                        "Name the agent: ghostex chief set-agent <agent-id> (ghostex chief status lists them).".into(),
                    )
                })?;
            change("setAgent", json!({ "agentId": agent }), &flags, json_output)
        }
        "send" => send(&parsed.rest, &parsed.flags, &flags),
        other => Err(CliError::Other(format!(
            "Unknown chief command: {other}. See ghostex chief --help."
        ))),
    }
}

/// `--server` and `--computer` pass through to the gxserver call.
fn target_flags(flags: &Flags) -> Flags {
    let mut result = Flags::default();
    for key in ["server", "computer"] {
        if let Some(value) = flags.string_value(key) {
            result.insert_text(key, value);
        }
    }
    result
}

fn call_chief(op: &str, mut params: Value, flags: &Flags) -> CliResult<Value> {
    params["op"] = json!(op);
    call_gxserver_rpc("/api/chief", &params, flags)
}

fn change(op: &str, params: Value, flags: &Flags, json_output: bool) -> CliResult<()> {
    let result = call_chief(op, params, flags)?;
    if json_output {
        print_json(&result);
        return Ok(());
    }
    let chief = &result["chief"];
    let text = |key: &str| chief[key].as_str().unwrap_or_default().to_string();
    match result["status"].as_str() {
        Some("existing") => println!(
            "Main Hub is already running on {} ({}:{}).",
            text("agentId"),
            text("projectId"),
            text("sessionId")
        ),
        Some("replaced") => println!(
            "Main Hub started a new chat on {}; its memory and threads came along.",
            text("agentId")
        ),
        _ => println!("Main Hub started on {}.", text("agentId")),
    }
    Ok(())
}

fn print_status(status: &Value) {
    if status["enabled"].as_bool() != Some(true) {
        println!("Main Hub is turned off. Turn it on in Settings > Extensions.");
        return;
    }
    match status.get("chief").filter(|chief| chief.is_object()) {
        Some(chief) => println!(
            "Main Hub runs on {} ({}).",
            chief["agentId"].as_str().unwrap_or_default(),
            status["globalRef"].as_str().unwrap_or_default()
        ),
        None => {
            println!("Main Hub hasn't been opened yet. Pick its agent with: ghostex chief open --agent <agent-id>");
            let agents: Vec<&str> = status["agents"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| row["agentId"].as_str())
                        .collect()
                })
                .unwrap_or_default();
            if !agents.is_empty() {
                println!("Agents it can run on: {}", agents.join(", "));
            }
        }
    }
}

/// Finds the target computer's Chief and messages it with `ghostex agents send`.
fn send(rest: &[String], raw: &Flags, flags: &Flags) -> CliResult<()> {
    let text = rest.join(" ");
    if text.trim().is_empty() {
        return Err(CliError::Other(
            "Write the message: ghostex chief send [--computer <name>] \"<text>\".".into(),
        ));
    }
    let status = call_chief("status", json!({}), flags)?;
    let where_ = raw
        .string_value("computer")
        .map(|name| format!(" on {name}"))
        .unwrap_or_default();
    if status["enabled"].as_bool() != Some(true) {
        return Err(CliError::Other(format!("Main Hub{where_} is turned off.")));
    }
    let reference = status["globalRef"]
        .as_str()
        .filter(|reference| !reference.is_empty())
        .ok_or_else(|| {
            CliError::Other(format!(
                "Main Hub{where_} hasn't been opened yet, so there is no one to send to."
            ))
        })?
        .to_string();
    let mut args = vec!["send".to_string(), reference, text];
    if let Some(server) = raw.string_value("server") {
        args.push("--server".into());
        args.push(server.to_string());
    }
    super::agents::run(&args)
}
