//! `ghostex hookless-activity [--reset]`: the side-by-side comparison of Claude, Codex and
//! OpenCode activity seen by their hooks and by Ghostex's hook-free observers
//! (crate::hookless_activity), as JSON.

use serde_json::json;

use super::args::parse_args;
use super::output::print_json;
use super::rpc::{self, CliResult};

pub(super) fn hookless_activity_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let result = rpc::call_gxserver_rpc(
        "/api/readHooklessActivityComparison",
        &json!({ "reset": flags.truthy("reset") }),
        flags,
    )?;
    print_json(&result);
    Ok(())
}
