use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::parse_args,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

pub fn tailcat_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("status") => status_command(&args[1..]),
        Some("enable") => set_enabled_command(&args[1..], true),
        Some("disable") => set_enabled_command(&args[1..], false),
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", crate::ghostex_cli::usage::tailcat_usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown tailcat command: {other}\n\n{}",
            crate::ghostex_cli::usage::tailcat_usage()
        ))),
    }
}

fn status_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    ensure_no_positional_args("tailcat status", &parsed.rest)?;
    let result = call_gxserver_rpc("/api/tailcatStatus", &json!({}), &parsed.flags)?;
    print_status(&result);
    Ok(())
}

fn set_enabled_command(args: &[String], enabled: bool) -> CliResult<()> {
    let parsed = parse_args(args);
    ensure_no_positional_args(
        if enabled {
            "tailcat enable"
        } else {
            "tailcat disable"
        },
        &parsed.rest,
    )?;
    let result = call_gxserver_rpc(
        "/api/updateTailcatState",
        &json!({ "kind": "setEnabled", "enabled": enabled }),
        &parsed.flags,
    )?;
    print_status(&result);
    Ok(())
}

/// The status object is the whole product of every verb, so all three print it
/// verbatim instead of inventing a second, drifting human summary.
fn print_status(result: &Value) {
    print_json(result.get("status").unwrap_or(result));
}

fn ensure_no_positional_args(command: &str, rest: &[String]) -> CliResult<()> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "{command} does not accept positional arguments."
        )))
    }
}
