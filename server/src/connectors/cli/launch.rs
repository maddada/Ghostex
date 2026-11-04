//! The flags that hand a Claude or Codex agent its workspace's `gh` and `gws` sign-ins: Claude
//! gets the variables as `--settings` `env`, Codex as `shell_environment_policy.set`, so every
//! command the agent runs uses the workspace's folders.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! Added to the command a launch or resume runs, never to the saved base command, only while Connectors is on and only for the tools the workspace signed in to, so a sign-in or sign-out reaches each session on its next start or resume. They go to the agent rather than the session's shell, whose environment comes from the session daemon's launch scripts; the user's own commands in that terminal keep their usual sign-in. A command that already names its own `--settings` is left alone, since Claude keeps only the last one.
//! SEE-ALSO: server/src/agents/launch_plan.rs (`create_agent_session_params_for_project`), server/src/agents/resume_plan.rs (`to_agent_resume_input`).

use serde_json::{json, Map, Value};

use crate::agents::quote_shell_arg;
use crate::paths::get_gxserver_paths;
use crate::workspaces::DEFAULT_WORKSPACE_ID;

use super::super::workspace::project_workspace;
use super::tools::session_env;

fn with_env(family: &str, env: &[(String, String)], command: &str) -> String {
    let command = command.trim_end();
    match family {
        "claude" if !command.contains("--settings") => {
            let vars: Map<String, Value> = env
                .iter()
                .map(|(name, value)| (name.clone(), json!(value)))
                .collect();
            let mut settings = json!({ "env": vars });
            // Claude keeps only the last `--settings`, so a bypass launch's consent rides in this one (agents/bypass_consent.rs).
            if crate::agents::claude_command_starts_in_bypass(command) {
                settings[crate::agents::CLAUDE_BYPASS_CONSENT_SETTING] = json!(true);
            }
            let settings = settings.to_string();
            format!("{command} --settings {}", quote_shell_arg(&settings))
        }
        "codex" => {
            let flags: Vec<String> = env
                .iter()
                .map(|(name, value)| {
                    format!(
                        "-c {}",
                        quote_shell_arg(&format!(
                            "shell_environment_policy.set.{name}={}",
                            serde_json::to_string(value).unwrap_or_default()
                        ))
                    )
                })
                .collect();
            format!("{command} {}", flags.join(" "))
        }
        _ => command.to_string(),
    }
}

/// `command` with the workspace sign-ins of `project`'s workspace, for a Claude or Codex agent.
pub(crate) fn with_project_cli_env(
    db: &rusqlite::Connection,
    project: &Value,
    family: Option<&str>,
    command: &str,
) -> String {
    // Unit tests assert exact commands, which must not depend on the user's sign-ins.
    if cfg!(test) || command.trim().is_empty() {
        return command.to_string();
    }
    let Some(family) = family.filter(|family| matches!(*family, "claude" | "codex")) else {
        return command.to_string();
    };
    let paths = get_gxserver_paths(None);
    if !super::super::connectors_enabled(&paths) {
        return command.to_string();
    }
    // Listing projects reads no server-specific field, so no server id is needed here.
    let repository = crate::domain::DomainRepository::new(db, "");
    let workspace = project_workspace(db, &repository, project)
        .unwrap_or_else(|_| DEFAULT_WORKSPACE_ID.to_string());
    let env = session_env(&paths, &workspace);
    if env.is_empty() {
        return command.to_string();
    }
    with_env(family, &env, command)
}

/// The same for a resume, which has no database open.
pub(crate) fn with_project_cli_env_for_resume(
    project: &Value,
    family: Option<&str>,
    command: String,
) -> String {
    if cfg!(test) {
        return command;
    }
    match crate::storage::open_gxserver_database(&get_gxserver_paths(None)) {
        Ok(db) => with_project_cli_env(&db, project, family, &command),
        Err(_) => command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bypass_launch_carries_its_consent_in_the_one_settings_flag() {
        let env = [("GH_CONFIG_DIR".to_string(), "/w/gh".to_string())];
        assert_eq!(
            with_env("claude", &env, "claude --dangerously-skip-permissions"),
            r#"claude --dangerously-skip-permissions --settings '{"env":{"GH_CONFIG_DIR":"/w/gh"},"skipDangerousModePermissionPrompt":true}'"#
        );
        assert_eq!(
            with_env("claude", &env, "claude"),
            r#"claude --settings '{"env":{"GH_CONFIG_DIR":"/w/gh"}}'"#
        );
    }
}
