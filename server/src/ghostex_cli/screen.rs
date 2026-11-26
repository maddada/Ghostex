//! `ghostex screen open [--machine <id>]`: a computer's screen in a window of its own in the app.
//!
//! CDXC:RemoteScreen 2026-11-27 WHY: the spike's window has no menu item yet (new UI waits for a
//! Live page the user approves), so this command is how it is opened. It asks the running app
//! (`openRemoteScreen`), which starts the stream on that computer's gxserver and shows it.
//! `--viewer-url` opens a viewer address gxserver already handed out, as it is, for a test copy
//! viewing a forwarded test gxserver.
use serde_json::{Map, Value};

use crate::ghostex_cli::actions::send_gxserver_cli_action;
use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};

const USAGE: &str = "Usage:
  ghostex screen open                      This computer's screen, in a window of its own
  ghostex screen open --machine <id>       A connected computer's screen
  ghostex screen open --viewer-url <url>   A viewer address gxserver handed out (test copies)

The computer must have Remote screen switched on (the remoteScreenEnabled setting).";

pub fn screen_command(args: &[String]) -> CliResult<()> {
    let (verb, rest) = args
        .split_first()
        .map(|(v, r)| (v.as_str(), r))
        .unwrap_or(("", &[]));
    match verb {
        "open" => open_command(rest),
        "" | "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown screen command \"{other}\".\n\n{USAGE}"
        ))),
    }
}

fn open_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let mut payload = Map::new();
    for (flag, key) in [("machine", "machineId"), ("viewer-url", "viewerUrl")] {
        if let Some(value) = parsed.flags.text(flag) {
            let value = value.trim();
            if !value.is_empty() {
                payload.insert(key.into(), Value::from(value));
            }
        }
    }
    let result =
        send_gxserver_cli_action("openRemoteScreen", &Value::Object(payload), &parsed.flags)?;
    let answer = result.get("result").unwrap_or(&result);
    if answer.get("ok").and_then(Value::as_bool) == Some(false) {
        let error = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The app did not open it.");
        return Err(CliError::Other(error.to_string()));
    }
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        println!("Opening the screen in its own window.");
    }
    Ok(())
}
