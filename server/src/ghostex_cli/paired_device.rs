use serde_json::json;

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/*
CDXC:RemotePairing 2026-09-03:
The mobile app has no gxserver credential of its own: it reaches this
computer over SSH with the key it registered at pairing time, and runs this
verb on every connect so Settings → Remote can show the device as
"connected now". It is one narrowly scoped bridge to `/api/pairedDeviceSeen`
on the local gxserver, not a general RPC escape hatch.
*/
fn usage() -> &'static str {
    "Usage: ghostex paired-device-seen --device-id <id> --json"
}

pub fn paired_device_seen_command(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    if !parsed.rest.is_empty() {
        return Err(CliError::Other(format!(
            "paired-device-seen does not accept positional arguments.\n\n{}",
            usage()
        )));
    }
    let device_id = parsed
        .flags
        .string_value("deviceId")
        .map(str::trim)
        .filter(|device_id| !device_id.is_empty())
        .ok_or_else(|| CliError::Other(format!("--device-id is required.\n\n{}", usage())))?;

    let result = call_gxserver_rpc(
        "/api/pairedDeviceSeen",
        &json!({ "deviceId": device_id }),
        &parsed.flags,
    )?;
    print_json(&result);
    if is_failed_cli_result(&result) {
        crate::ghostex_cli::set_exit_code(1);
    }
    Ok(())
}
