//! `ghostex security log`: read this computer's security log and check that nobody changed it
//! (`/api/securityLog`, server/src/audit/).

use serde_json::{json, Value};

use super::args::Flags;
use super::output::print_json;
use super::rpc::{call_gxserver_rpc, CliError, CliResult};

const HELP: &str = "Usage: ghostex security log [--last <n>] [--json]

Ghostex records every sensitive action in a security log: pairing a phone, Easy Connect changes,
sharing a chat in Collab, letting a connected computer's agents in, Autopilot answering an
approval, and attempts to change settings only you can change. Each entry says who asked, what,
when and how it ended, and is chained to the one before it, so a changed or removed entry shows.
Settings > Security shows the same list.

  ghostex security log               The newest 20 entries and whether the log is intact
  ghostex security log --last <n>    The newest <n> entries
  ghostex security log --json        The entries and the check as JSON

Exits with code 2 when the log was changed.";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{HELP}");
        return Ok(());
    }
    if args[0] != "log" {
        return Err(CliError::Other(format!(
            "Unknown security command \"{}\". Run ghostex security --help.",
            args[0]
        )));
    }
    let json_out = args.iter().any(|arg| arg == "--json");
    let last = args
        .iter()
        .position(|arg| arg == "--last")
        .and_then(|at| args.get(at + 1))
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| CliError::Other("--last takes a number.".to_string()))
        })
        .transpose()?
        .unwrap_or(20);
    let result = call_gxserver_rpc(
        crate::audit::SECURITY_LOG_ENDPOINT,
        &json!({ "limit": last }),
        &Flags::default(),
    )?;
    let intact = result
        .pointer("/check/intact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if json_out {
        print_json(&result);
    } else {
        print!("{}", render(&result));
    }
    if !intact {
        super::set_exit_code(2);
    }
    Ok(())
}

/// The plain-text view: the check first, then one line per entry, newest first.
pub(crate) fn render(result: &Value) -> String {
    let mut out = String::new();
    let check = &result["check"];
    let count = check["entries"].as_u64().unwrap_or(0);
    if check["intact"].as_bool() == Some(true) {
        out.push_str(&format!("Security log intact: {count} entries.\n"));
    } else {
        let message = check
            .pointer("/problem/message")
            .and_then(Value::as_str)
            .unwrap_or("The log could not be checked.");
        out.push_str(&format!(
            "WARNING: the security log was changed. {message}\n"
        ));
    }
    let entries = result["entries"].as_array().cloned().unwrap_or_default();
    if entries.is_empty() {
        out.push_str("No sensitive actions recorded yet.\n");
        return out;
    }
    for entry in entries {
        let at = entry["atMs"]
            .as_i64()
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|at| {
                at.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let text = |key: &str| entry[key].as_str().unwrap_or_default().to_string();
        let mut line = format!(
            "#{} {at}  {}  [{}]  by {}",
            entry["seq"].as_u64().unwrap_or(0),
            text("summary"),
            text("outcome"),
            entry
                .pointer("/actor/label")
                .and_then(Value::as_str)
                .unwrap_or("?"),
        );
        if let Some(target) = entry["target"].as_str() {
            line.push_str(&format!("  ({target})"));
        }
        if let Some(reason) = entry["reason"].as_str() {
            line.push_str(&format!("  {reason}"));
        }
        if let Some(approval) = entry.pointer("/approval/id").and_then(Value::as_str) {
            line.push_str(&format!("  approval {approval}"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}
