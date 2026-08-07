use serde_json::{Map, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/*
CDXC:BoardStartWork 2026-08-07:
`ghostex board start-work <bead-id>` is the CLI face of gxserver's
`/api/startBoardWork`: a thin wrapper like `run-action` and `create-agent`,
so external orchestrators dispatch a Project Board card through the daemon
instead of launching a worker session themselves. The daemon owns bead
resolution, the idempotent reuse guard, the canonical work prompt, and the
conversation link; the CLI only ships the parameters and prints the JSON
result (`{ sessionId, created }`).
*/
pub fn board_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("start-work") => start_work_command(&args[1..]),
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", crate::ghostex_cli::usage::board_usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown board command: {other}\n\n{}",
            crate::ghostex_cli::usage::board_usage()
        ))),
    }
}

fn start_work_command(args: &[String]) -> CliResult<()> {
    if matches!(
        args.first().map(String::as_str),
        Some("help") | Some("-h") | Some("--help")
    ) {
        println!("{}", crate::ghostex_cli::usage::board_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let bead_id = parsed
        .flags
        .text("beadId")
        .or_else(|| parsed.rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    if bead_id.is_empty() {
        return Err(CliError::Other(
            "board start-work requires a bead id.".to_string(),
        ));
    }
    let mut payload = Map::new();
    payload.insert("beadId".to_string(), Value::String(bead_id));
    if let Some(agent) = parsed
        .flags
        .text("agent")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert("agent".to_string(), Value::String(agent));
    }
    if let Some(project_id) = parsed
        .flags
        .text("projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert("projectId".to_string(), Value::String(project_id));
    }
    let result = call_gxserver_rpc(
        "/api/startBoardWork",
        &Value::Object(payload),
        &parsed.flags,
    )?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}
