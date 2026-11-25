//! `ghostex shown-command open <id>`: Run in terminal for a command Ghostex shows, from a shell or
//! the phone (which reaches a computer only through this CLI). gxserver resolves the id, makes the
//! Main Hub terminal and types the command there without Enter (server/src/shown_commands/).
use serde_json::json;

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

fn usage() -> String {
    "Ghostex shown-command - open a Main Hub terminal with a command Ghostex shows typed in

Usage:
  ghostex shown-command open <id> [--json]

Ids:
  linuxUpdate   The Linux update command Settings > Updates shows (only while an update is out)
  toolInstall:<trycua|spaceo|hex>
                The tool's install command (offered after its install failed)
  agentCli:<agent id>
                The command of that agent CLI's last install or update (only after it failed)

The command is typed at the new terminal's prompt and not run: press Enter there.
"
    .to_string()
}

pub fn shown_command_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("open") => {
            let parsed = parse_args(&args[1..]);
            let id = parsed.rest.first().map(|id| id.trim()).unwrap_or_default();
            if id.is_empty() {
                return Err(CliError::Other(format!("Name the command.\n\n{}", usage())));
            }
            let result = call_gxserver_rpc(
                "/api/openShownCommandTerminal",
                &json!({ "commandId": id }),
                &parsed.flags,
            )?;
            if parsed.flags.truthy("json") {
                print_json(&result);
            } else {
                println!(
                    "Opened \"{}\" in Main Hub with the command typed in; press Enter there to run it.",
                    result["title"].as_str().unwrap_or("Terminal")
                );
            }
            Ok(())
        }
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown shown-command command: {other}\n\n{}",
            usage()
        ))),
    }
}
