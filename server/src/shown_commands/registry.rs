//! The commands Ghostex itself shows a user to run, each known by an id gxserver resolves.
//!
//! CDXC:ServerApi 2026-11-25 WHY: Run in terminal types one of these into a Main Hub terminal (terminal.rs). A client names only the id (and, for a few, checked arguments such as a version or a workspace) and gxserver works out the text itself, so no client (another computer, the phone, an agent with a session token) can have Ghostex type text of its own choosing into a terminal. To add a command, add its id here with the terminal's title and the function that returns the text Ghostex shows for it now, then show Run in terminal wherever that command is shown, with this id, only when the status that carries the command also carries the id (an older gxserver has no such id). Arguments are never pasted into the text as they come: each is checked against what gxserver knows (a plain release version, a workspace it has, an agentbox step's own validation) or quoted for the shell.
use serde_json::{Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

/// The Linux update command Settings > Updates and the update dialog show
/// (app_update/linux_install.rs).
pub(crate) const LINUX_UPDATE: &str = "linuxUpdate";
/// Settings > Updates' Go back row and the Go back dialog on a deb or rpm install; `version`.
pub(crate) const LINUX_GO_BACK: &str = "linuxGoBack";
/// `gh auth login`: clone errors, source control sign-in, the Work page's GitHub notices.
pub(crate) const GH_AUTH_LOGIN: &str = "ghAuthLogin";
/// `glab auth login`: source control sign-in for GitLab.
pub(crate) const GLAB_AUTH_LOGIN: &str = "glabAuthLogin";
/// `gh auth refresh -s read:project`: GitHub Projects without the scope (work_mode/github_projects.rs).
pub(crate) const GH_PROJECTS_SCOPE: &str = "ghProjectsScope";
/// `gws auth setup`: Google Workspace's OAuth client (connectors/cli/client.rs).
pub(crate) const GWS_AUTH_SETUP: &str = "gwsAuthSetup";
/// `gemini-api configure`: the Gemini API CLI explainer's key step (Settings > Integrations > Tools).
pub(crate) const GEMINI_API_CONFIGURE: &str = "geminiApiConfigure";
/// `ghostex team deploy --workspace <name>`; `workspaceId`.
pub(crate) const TEAM_DEPLOY: &str = "teamDeploy";
/// `ghostex team slack-connect --workspace <name>`; `workspaceId`.
pub(crate) const TEAM_SLACK_CONNECT: &str = "teamSlackConnect";
/// `ghostex team linear-connect --workspace <name>`; `workspaceId`.
pub(crate) const TEAM_LINEAR_CONNECT: &str = "teamLinearConnect";
/// Turn on SSH access's Linux fallback: the OpenSSH install line, or the `sudo systemctl` line
/// that starts it (remote_access/ssh_enable.rs `manual_ssh_command`).
pub(crate) const SSH_SERVER: &str = "sshServer";
/// `ssh [-p <port>] user@host`: the sign-in a remote computer set to reuse the user's own SSH
/// connection asks for (apps/desktop/src/app/helpers/remote/ssh_connection_share.rs); `machineId`.
pub(crate) const SSH_SIGN_IN: &str = "sshSignIn";
/// Settings > Accounts' Claude Swap install command (accounts/model.rs).
pub(crate) const CLAUDE_SWAP_INSTALL: &str = "claudeSwapInstall";
/// Settings > Accounts' Codex Swap install command (accounts/model.rs).
pub(crate) const CODEX_SWAP_INSTALL: &str = "codexSwapInstall";
/// `bun upgrade`: an agent CLI that needs a newer Bun (agent_cli/catalog.rs).
pub(crate) const BUN_UPGRADE: &str = "bunUpgrade";
/// Linux system tools' `sudo` install where there is no password dialog
/// (managed_tools/system_tools.rs `terminal_command`).
pub(crate) const SYSTEM_TOOLS_INSTALL: &str = "systemToolsInstall";
/// One Settings > Cloud Boxes step (agentbox/command.rs `terminal_command`); its `command` and
/// arguments (`provider`, `agent`, `host`, `alias`, `ssh`).
pub(crate) const AGENTBOX_STEP: &str = "agentboxStep";

pub(crate) struct ShownCommand {
    /// The new terminal's title in the sidebar.
    pub(crate) title: String,
    pub(crate) command: String,
}

impl ShownCommand {
    fn new(title: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            command: command.into(),
        }
    }
}

pub(crate) async fn resolve(
    state: &AppState,
    id: &str,
    args: &Map<String, Value>,
) -> Result<ShownCommand, DomainStateError> {
    let arg = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let home = state.paths.agent_config_home_dir().to_path_buf();
    // A failed install's command (installs.rs).
    if let Some(resolved) = super::installs::resolve(state, id) {
        return resolved;
    }
    match id {
        LINUX_UPDATE => {
            let command = crate::app_update::shown_update_command(state)
                .await
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "There is no Ghostex update to install on this computer right now.",
                    )
                })?;
            Ok(ShownCommand::new("Update Ghostex", command))
        }
        LINUX_GO_BACK => {
            let version = arg("version").unwrap_or_default();
            let command = crate::app_update::go_back_command(&version).ok_or_else(|| {
                DomainStateError::bad_request(
                    "Ghostex can't go back to that version with a command on this computer.",
                )
            })?;
            Ok(ShownCommand::new(
                format!("Go back to {}", version.trim_start_matches('v')),
                command,
            ))
        }
        GH_AUTH_LOGIN => Ok(ShownCommand::new("Sign in to GitHub", "gh auth login")),
        GLAB_AUTH_LOGIN => Ok(ShownCommand::new("Sign in to GitLab", "glab auth login")),
        GH_PROJECTS_SCOPE => Ok(ShownCommand::new(
            "Allow GitHub Projects",
            crate::work_mode::GITHUB_PROJECTS_SCOPE_COMMAND,
        )),
        GWS_AUTH_SETUP => Ok(ShownCommand::new(
            "Set up Google Workspace",
            "gws auth setup",
        )),
        GEMINI_API_CONFIGURE => Ok(ShownCommand::new(
            "Add your Gemini API key",
            crate::managed_tools::GEMINI_API_CONFIGURE_COMMAND,
        )),
        TEAM_DEPLOY | TEAM_SLACK_CONNECT | TEAM_LINEAR_CONNECT => {
            let (verb, title) = match id {
                TEAM_DEPLOY => ("deploy", "Deploy the team"),
                TEAM_SLACK_CONNECT => ("slack-connect", "Connect Slack"),
                _ => ("linear-connect", "Connect Linear"),
            };
            let name = team_workspace_name(state, arg("workspaceId")).await?;
            Ok(ShownCommand::new(title, team_command(verb, &name)))
        }
        SSH_SERVER => {
            let command = tokio::task::spawn_blocking(crate::remote_access::manual_ssh_command)
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Ghostex has no command to turn on SSH access on this computer.",
                    )
                })?;
            Ok(ShownCommand::new("Turn on SSH access", command))
        }
        SSH_SIGN_IN => {
            let machine_id = arg("machineId").unwrap_or_default();
            let settings = crate::shared_settings_file::read_shared_settings(&state.paths);
            let command = ssh_sign_in_command(settings.as_ref(), &machine_id).ok_or_else(|| {
                DomainStateError::bad_request(
                    "That computer doesn't reuse an SSH connection, so there is nothing to sign in to.",
                )
            })?;
            Ok(ShownCommand::new("Sign in over SSH", command))
        }
        CLAUDE_SWAP_INSTALL => Ok(ShownCommand::new(
            "Install Claude Swap",
            crate::accounts::model::Provider::Claude.install_command(&home),
        )),
        CODEX_SWAP_INSTALL => Ok(ShownCommand::new(
            "Install Codex Swap",
            crate::accounts::model::Provider::Codex.install_command(&home),
        )),
        BUN_UPGRADE => Ok(ShownCommand::new("Update Bun", "bun upgrade")),
        SYSTEM_TOOLS_INSTALL => {
            let command = tokio::task::spawn_blocking(move || {
                crate::managed_tools::system_tools::terminal_command(&home)
            })
            .await
            .ok()
            .flatten()
            .ok_or_else(|| {
                DomainStateError::bad_request(
                    "This computer has the system tools, or installs them without a terminal.",
                )
            })?;
            Ok(ShownCommand::new("Install system tools", command))
        }
        AGENTBOX_STEP => {
            // agentbox's own check: a named step with validated arguments, never free text.
            let mut params = Map::new();
            params.insert("action".into(), "terminalCommand".into());
            for key in ["command", "provider", "agent", "host", "alias", "ssh"] {
                if let Some(value) = arg(key) {
                    params.insert(key.into(), value.into());
                }
            }
            let answer = crate::agentbox::agentbox_terminal_command(&params)?;
            let command = answer["command"]
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| {
                    DomainStateError::bad_request("That Cloud Boxes step has no command.")
                })?;
            let title = answer["title"]
                .as_str()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("agentbox");
            Ok(ShownCommand::new(title, command))
        }
        _ => Err(no_command(id)),
    }
}

/// Why an id gives no command (an unknown id, or one with nothing to type on this computer now).
fn no_command(id: &str) -> DomainStateError {
    DomainStateError::bad_request(format!("Ghostex has no command called \"{id}\" to run."))
}

/// The name of a workspace this computer has, for the `ghostex team` verbs.
async fn team_workspace_name(
    state: &AppState,
    workspace_id: Option<String>,
) -> Result<String, DomainStateError> {
    let workspace_id =
        workspace_id.ok_or_else(|| DomainStateError::bad_request("Name the team's workspace."))?;
    let paths = state.paths.clone();
    tokio::task::spawn_blocking(move || {
        let db =
            crate::storage::open_gxserver_database(&paths).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("The workspace could not be read: {error}"),
            })?;
        let workspaces = crate::workspaces::read_sidebar_workspaces(&db)?;
        let id = crate::workspaces::find_workspace_id(&workspaces, &workspace_id)
            .ok_or_else(|| DomainStateError::bad_request(crate::workspaces::NO_SUCH_WORKSPACE))?;
        Ok(crate::workspaces::workspace_name(&workspaces, &id))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?
}

/// `ghostex team <verb> --workspace <name>`, the name in single quotes unless it is a plain word.
/// SEE-ALSO: `team_command` in apps/desktop/src/app/window/settings_modal/tabs/workspaces/team.rs
/// shows the same text.
pub(crate) fn team_command(verb: &str, workspace_name: &str) -> String {
    let plain = !workspace_name.is_empty()
        && workspace_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    let name = if plain {
        workspace_name.to_string()
    } else {
        format!("'{}'", workspace_name.replace('\'', "'\\''"))
    };
    format!("ghostex team {verb} --workspace {name}")
}

/// `ssh [-p <port>] user@host` for a saved remote computer that reuses the user's own SSH
/// connection, as the desktop shows it (`gpui_remote_ssh_sign_in_hint`), or `None`. The host and
/// user come from Settings, never from the request: a target that could read as an ssh option is
/// refused, and one with characters a shell would read is single-quoted.
fn ssh_sign_in_command(settings: Option<&Value>, machine_id: &str) -> Option<String> {
    let machine = settings?
        .get("remoteMachines")?
        .as_array()?
        .iter()
        .find(|machine| {
            machine.get("id").and_then(Value::as_str).map(str::trim) == Some(machine_id.trim())
        })?;
    let text = |key: &str| {
        machine
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let reuses = machine.get("reuseSshConnection").and_then(Value::as_bool) == Some(true)
        && machine.get("transport").and_then(Value::as_str) != Some("easyConnect")
        && text("wslDistribution").is_none();
    if !reuses || machine_id.trim().is_empty() {
        return None;
    }
    let host = text("sshHost")?;
    let target = match text("sshUser") {
        Some(user) => format!("{user}@{host}"),
        None => host.to_string(),
    };
    if target.starts_with('-') {
        return None;
    }
    let plain = target.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | ':' | '[' | ']' | '%')
    });
    let target = if plain {
        target
    } else {
        format!("'{}'", target.replace('\'', "'\\''"))
    };
    let port = machine
        .get("sshPort")
        .and_then(Value::as_u64)
        .filter(|port| (1..=u16::MAX as u64).contains(port));
    Some(match port {
        Some(port) => format!("ssh -p {port} {target}"),
        None => format!("ssh {target}"),
    })
}
