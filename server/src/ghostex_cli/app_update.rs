//! `ghostex app-update`: read, check, install and the keep-up-to-date switch of a computer's Ghostex app (`--server <profile>` for another computer).
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::parse_args,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// How long `install --wait` follows an install, through the restart, before it gives up.
const INSTALL_WAIT_LIMIT: Duration = Duration::from_secs(20 * 60);
const INSTALL_WAIT_POLL: Duration = Duration::from_secs(2);

pub fn app_update_command(args: &[String]) -> CliResult<()> {
    let usage = crate::ghostex_cli::usage::app_update_usage;
    match args.first().map(String::as_str) {
        Some("status") => simple_command(&args[1..], "/api/appUpdateStatus", json!({})),
        Some("check") => simple_command(&args[1..], "/api/appUpdateCheck", json!({})),
        Some("install") => install_command(&args[1..]),
        Some("auto") => auto_command(&args[1..]),
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown app-update command: {other}\n\n{}",
            usage()
        ))),
    }
}

fn simple_command(args: &[String], path: &str, params: Value) -> CliResult<()> {
    let parsed = parse_args(args);
    let result = call_gxserver_rpc(path, &params, &parsed.flags)?;
    print_json(result.get("status").unwrap_or(&result));
    Ok(())
}

fn auto_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let enabled = match parsed.rest.first().map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => {
            return Err(CliError::Other(
                "Usage: ghostex app-update auto on|off".to_string(),
            ))
        }
    };
    let result = call_gxserver_rpc(
        "/api/appUpdateSetAuto",
        &json!({ "enabled": enabled }),
        &parsed.flags,
    )?;
    print_json(result.get("status").unwrap_or(&result));
    Ok(())
}

fn install_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let result = call_gxserver_rpc("/api/appUpdateInstall", &json!({}), &parsed.flags)?;
    let started = result.get("status").cloned().unwrap_or(result);
    if !parsed.flags.truthy("wait") {
        print_json(&started);
        return Ok(());
    }
    let from_version = started
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let deadline = Instant::now() + INSTALL_WAIT_LIMIT;
    let mut last = started;
    while Instant::now() < deadline {
        std::thread::sleep(INSTALL_WAIT_POLL);
        // The computer's gxserver restarts during the install, so a failed read is waited out.
        let Ok(result) = call_gxserver_rpc("/api/appUpdateStatus", &json!({}), &parsed.flags)
        else {
            continue;
        };
        last = result.get("status").cloned().unwrap_or(result);
        let finished = last.get("pendingInstall").is_none_or(Value::is_null)
            && (last
                .get("lastResult")
                .is_some_and(|result| !result.is_null())
                || last.get("version").and_then(Value::as_str) != Some(from_version.as_str()));
        if finished {
            print_json(&last);
            return Ok(());
        }
    }
    print_json(&last);
    Err(CliError::Other(
        "The update did not finish in time; run `ghostex app-update status` to see where it stopped."
            .to_string(),
    ))
}
