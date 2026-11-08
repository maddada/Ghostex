//! `ghostex gbots`: Ghostex's bots (server/src/gbots/). The work happens in gxserver
//! (`/api/gbots`).

use serde_json::json;

use crate::ghostex_cli::{
    args::parse_args,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const HELP: &str = "GBots: your own team of bots, led by Chief of Staff in Main Hub.

USAGE
  ghostex gbots list [--json]

  list shows your GBots. Turn on Main Hub and GBots in Settings > Extensions.
  Add --computer <name> to run a command on another computer.
";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    match parsed.rest[0].as_str() {
        "list" => {
            let answer = call_gxserver_rpc("/api/gbots", &json!({ "op": "list" }), &flags)?;
            if flags.contains("json") {
                print_json(&answer);
                return Ok(());
            }
            let bots = answer["bots"].as_array().cloned().unwrap_or_default();
            if bots.is_empty() {
                println!("No GBots yet.");
            }
            for bot in bots {
                println!(
                    "{}\t{}",
                    bot["name"].as_str().unwrap_or_default(),
                    bot["botId"].as_str().unwrap_or_default()
                );
            }
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "unknown gbots command `{other}`.\n\n{HELP}"
        ))),
    }
}
