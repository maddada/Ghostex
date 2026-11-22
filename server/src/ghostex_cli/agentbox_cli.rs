//! `ghostex agentbox`: agentbox readiness, the box list, and a box session's web app or screen URL.
//! The work happens in gxserver (`/api/agentbox`, server/src/agentbox/), the same calls Settings >
//! Cloud Boxes makes.

use crate::ghostex_cli::{
    args::parse_args,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};
use serde_json::{json, Value};

const HELP: &str = "Run agent sessions in agentbox boxes (Docker on this computer, or a cloud).

USAGE
  ghostex agentbox status [--refresh] [--json]
  ghostex agentbox list [--json]
  ghostex agentbox url <session> [--screen] [--json]
  ghostex agentbox url --box <box-name> [--screen] [--json]
  ghostex create-agent <agent> --project-id <id> --run-on <location>

  status shows whether agentbox is installed and which locations can start a box
  now. list shows every box with the Ghostex session it belongs to. url prints a
  box's web app address on this computer (--screen: the box's own screen in a
  browser). <session> is a title, session id or global ref, as in `ghostex sessions`.

  Run locations for --run-on: local, docker, hetzner, vercel, daytona, e2b,
  digitalocean, or docker:<host> for your own server added with
  `agentbox remote-docker add <host> <user@server>`. Boxes run Claude, Codex,
  OpenCode and Pi.

EXAMPLES
  ghostex agentbox status
  ghostex create-agent codex --project-id P1 --run-on docker
  ghostex agentbox url \"Fix the header\"
";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    // Box commands run the agentbox CLI, and `url` on a cloud box opens an SSH forward first.
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", "75000");
    }
    let json_output = flags.contains("json");
    let request = |params: Value| call_gxserver_rpc("/api/agentbox", &params, &flags);
    match parsed.rest[0].as_str() {
        "status" => {
            let status = request(json!({
                "action": "status",
                "refresh": flags.contains("refresh"),
            }))?;
            if json_output {
                print_json(&status);
            } else {
                print_status(&status);
            }
        }
        "list" | "ls" => {
            let result = request(json!({ "action": "list" }))?;
            if json_output {
                print_json(&result);
                return Ok(());
            }
            let boxes = result["boxes"].as_array().cloned().unwrap_or_default();
            if boxes.is_empty() {
                println!("No boxes.");
                return Ok(());
            }
            println!("BOX\tAGENT\tPROVIDER\tSTATE\tACTIVITY\tSESSION");
            for listed in boxes {
                let session = match (listed["projectId"].as_str(), listed["sessionId"].as_str()) {
                    (Some(project), Some(session)) => format!("{project}/{session}"),
                    _ => "-".to_string(),
                };
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{session}",
                    text(&listed, "name"),
                    text(&listed, "agent"),
                    text(&listed, "provider"),
                    text(&listed, "state"),
                    text(&listed, "activity"),
                );
            }
        }
        "url" => {
            let target = if flags.contains("screen") {
                "screen"
            } else {
                "web"
            };
            let params = match flags.string_value("box") {
                Some(box_name) => {
                    json!({ "action": "openTarget", "boxName": box_name, "target": target })
                }
                None => {
                    let reference = parsed.rest.get(1).ok_or_else(|| {
                        CliError::Other(format!(
                            "agentbox url needs a session or --box <box-name>.\n\n{HELP}"
                        ))
                    })?;
                    let rows = sessions::fetch_session_list(&flags, false)?;
                    let session = selector::resolve_one_listed_session(reference, &rows, &flags)?;
                    json!({
                        "action": "openTarget",
                        "projectId": session["projectId"],
                        "sessionId": session["sessionId"],
                        "target": target,
                    })
                }
            };
            let result = request(params)?;
            if json_output {
                print_json(&result);
            } else {
                println!("{}", text(&result, "url"));
            }
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown agentbox command: {other}\n\n{HELP}"
            )))
        }
    }
    Ok(())
}

fn text(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .filter(|text| !text.is_empty())
        .unwrap_or("-")
        .to_string()
}

fn print_status(status: &Value) {
    if status["supported"] == false {
        println!("agentbox runs on macOS and Linux only.");
        return;
    }
    if status["installed"] != true {
        println!("agentbox is not installed.");
        println!("Install it with: npm install -g @madarco/agentbox && agentbox install");
        return;
    }
    println!(
        "agentbox {} ({})",
        text(status, "version"),
        text(status, "binaryPath")
    );
    if let Some(error) = status["error"].as_str() {
        println!("Check failed: {error}");
    }
    println!(
        "Docker: {}",
        if status["dockerReady"] == true {
            "ready"
        } else {
            "not reachable"
        }
    );
    println!(
        "Box sign-in: Claude {}, Codex {}",
        yes_no(&status["agentSignIns"]["claude"]),
        yes_no(&status["agentSignIns"]["codex"])
    );
    println!();
    println!("LOCATION\tREADY\tDETAIL");
    for provider in status["providers"].as_array().into_iter().flatten() {
        let id = text(provider, "id");
        let location = if provider["kind"] == "remoteDocker" && id == "remote-docker" {
            "docker:<host>".to_string()
        } else {
            id
        };
        let detail = provider["hint"]
            .as_str()
            .or_else(|| provider["detail"].as_str())
            .unwrap_or("");
        println!(
            "{location}\t{}\t{detail}",
            if provider["ready"] == true {
                "yes"
            } else {
                "no"
            }
        );
    }
}

fn yes_no(value: &Value) -> &'static str {
    if value == &Value::Bool(true) {
        "signed in"
    } else {
        "not signed in"
    }
}
