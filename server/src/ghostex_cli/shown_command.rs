//! `ghostex shown-command open <id>`: Run in terminal for a command Ghostex shows, from a shell or
//! the phone (which reaches a computer only through this CLI). gxserver resolves the id, makes the
//! Main Hub terminal and types the command there without Enter (server/src/shown_commands/).
use serde_json::json;

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/// The arguments some ids take (server/src/shown_commands/registry.rs checks each).
const ARG_FLAGS: [&str; 9] = [
    "version",
    "workspaceId",
    "machineId",
    "command",
    "provider",
    "agent",
    "host",
    "alias",
    "ssh",
];

fn usage() -> String {
    "Ghostex shown-command - open a Main Hub terminal with a command Ghostex shows typed in

Usage:
  ghostex shown-command open <id> [--version <v>] [--workspace-id <id>] [--json]

Ids:
  linuxUpdate          The Linux update command Settings > Updates shows (only while an update is out)
  toolInstall:<trycua|spaceo|hex>
                       The tool's install command (offered after its install failed)
  agentCli:<agent id>
                       The command of that agent CLI's last install or update (only after it failed)
  linuxGoBack          Go back to an older version on a deb or rpm install (--version)
  ghAuthLogin          gh auth login
  glabAuthLogin        glab auth login
  ghProjectsScope      gh auth refresh -s read:project (GitHub Projects)
  gwsAuthSetup         gws auth setup (Google Workspace)
  geminiApiConfigure   gemini-api configure (add a Gemini API key)
  teamDeploy           ghostex team deploy (--workspace-id)
  teamSlackConnect     ghostex team slack-connect (--workspace-id)
  teamLinearConnect    ghostex team linear-connect (--workspace-id)
  sshServer            Install or start the OpenSSH server (Linux)
  sshSignIn            ssh to a remote computer that reuses your SSH connection (--machine-id)
  claudeSwapInstall    Install Claude Swap
  codexSwapInstall     Install Codex Swap
  bunUpgrade           bun upgrade
  systemToolsInstall   Install Linux system tools with sudo
  agentboxStep         A Cloud Boxes step (--command, --provider, --agent, --host, --alias, --ssh)

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
            let mut command_args = serde_json::Map::new();
            for key in ARG_FLAGS {
                if let Some(value) = parsed.flags.string_value(key) {
                    command_args.insert(key.to_string(), json!(value));
                }
            }
            let result = call_gxserver_rpc(
                "/api/openShownCommandTerminal",
                &json!({ "commandId": id, "args": command_args }),
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
