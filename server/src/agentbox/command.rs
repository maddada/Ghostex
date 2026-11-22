//! The `agentbox …` command text Ghostex runs: box launch, re-attach, and the setup commands the
//! Cloud Boxes page runs in a command pane.

use serde_json::{json, Map, Value};

use super::location::{
    is_cloud_provider, is_valid_alias, is_valid_ssh_target, provider_label, provider_spec,
};
use crate::agents::{as_atuin_ignored_shell_input, quote_shell_arg};
use crate::domain::DomainStateError;

/// One shell word: plain words stay bare, everything else is single-quoted (newlines included,
/// which `zsh`'s `eval` and `sh -c` both take inside quotes).
fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._:/=@%+,".contains(&byte))
    {
        value.to_string()
    } else {
        quote_shell_arg(value)
    }
}

/// The provider positional of `agentbox [<provider>] <agent>`: none for local Docker.
fn provider_positional(provider: &str) -> Option<&str> {
    (provider != "docker").then_some(provider)
}

/// `agentbox [<provider>] <agent> --inline -y -n <box> [-- <agent args…> [<first prompt>]]`.
///
/// CDXC:AgentBox 2026-10-01 WHY: the box's agent runs inside a normal local zmx session whose command is this inline agentbox client, so terminal attach, screen reading, `zmx history`, sending text and sleep/wake keep working unchanged. A first prompt rides the launch as the agent's own initial prompt, so it reaches the boxed agent exactly once; the matching startup send is answered without typing it (see `first_prompt.rs`).
pub(crate) fn box_launch_command(
    provider: &str,
    agent: &str,
    box_name: &str,
    agent_args: &[String],
    first_prompt: Option<&str>,
) -> String {
    let mut words = vec!["agentbox".to_string()];
    if let Some(provider) = provider_positional(provider) {
        words.push(shell_word(provider));
    }
    words.push(agent.to_string());
    words.extend(["--inline", "-y", "-n"].map(str::to_string));
    words.push(shell_word(box_name));
    let mut inner: Vec<String> = agent_args.iter().map(|arg| shell_word(arg)).collect();
    if let Some(prompt) = first_prompt.filter(|prompt| !prompt.trim().is_empty()) {
        // OpenCode's positional is a project folder; its first message is `--prompt`. Claude,
        // Codex and Pi take it after `--`, so a prompt such as "-v is broken" stays a prompt.
        inner.push(
            if agent == "opencode" {
                "--prompt"
            } else {
                "--"
            }
            .to_string(),
        );
        inner.push(quote_shell_arg(prompt));
    }
    if !inner.is_empty() {
        words.push("--".to_string());
        words.extend(inner);
    }
    words.join(" ")
}

/// `agentbox <agent> attach <box> --inline`: re-attaches, starting the agent's session and the
/// box itself when either is not running.
pub(crate) fn box_attach_command(agent: &str, box_name: &str) -> String {
    format!("agentbox {agent} attach {} --inline", shell_word(box_name))
}

/// The launch plan of a box session, in the shape `build_agent_launch_plan` returns.
pub(crate) fn box_launch_plan(
    base_command: &str,
    command: &str,
    first_user_message: Option<&str>,
) -> Value {
    let mut plan = Map::new();
    plan.insert("agentCommand".to_string(), json!(base_command));
    plan.insert("command".to_string(), json!(command));
    if let Some(message) = first_user_message.filter(|message| !message.trim().is_empty()) {
        plan.insert("firstUserMessage".to_string(), json!(message));
    }
    plan.insert(
        "startupText".to_string(),
        json!(as_atuin_ignored_shell_input(command)),
    );
    plan.insert(
        "startupTextDisposition".to_string(),
        json!("queueAfterTerminalReady"),
    );
    Value::Object(plan)
}

/// POSIX shell words of a single plain command, or `None` when it uses quoting the box launch
/// cannot carry over (operators, substitutions, comments, unbalanced quotes).
fn split_plain_command(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = command.trim().chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        other => current.push(other),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            escaped @ ('"' | '\\' | '$' | '`') => current.push(escaped),
                            other => {
                                current.push('\\');
                                current.push(other);
                            }
                        },
                        '$' | '`' => return None,
                        other => current.push(other),
                    }
                }
            }
            '\\' => {
                in_word = true;
                current.push(chars.next()?);
            }
            ';' | '&' | '|' | '<' | '>' | '(' | ')' | '`' | '$' | '#' => return None,
            ch if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            other => {
                in_word = true;
                current.push(other);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    Some(words)
}

/// The arguments the agent's own command line carries (model, effort and custom flags), taken from
/// the command gxserver built before account wrapping and accept-all flags.
///
/// A command that wraps the agent in something else (an env prefix, a launcher, shell operators)
/// carries nothing over: only words after the agent binary itself are meaningful inside the box.
pub(crate) fn box_agent_args(command: Option<&str>, agent: &str) -> Vec<String> {
    let Some(words) = command.and_then(split_plain_command) else {
        return Vec::new();
    };
    let Some(binary_index) = words.iter().position(|word| {
        word.rsplit('/').next() == Some(agent)
            || (agent == "claude" && word.rsplit('/').next() == Some("claude-code"))
    }) else {
        return Vec::new();
    };
    words
        .into_iter()
        .skip(binary_index + 1)
        // agentbox already launches Claude without permission prompts inside the box.
        .filter(|word| !(agent == "claude" && word == "--dangerously-skip-permissions"))
        .collect()
}

fn text_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `{"action":"terminalCommand", …}` → `{"title", "command"}`.
///
/// CDXC:AgentBox 2026-10-01 WHY: gxserver owns the command text, like `runManagedToolTerminalCommand`, so a page can only ask for one of these named commands with validated arguments and can never make the app run text of its choosing.
pub(crate) fn terminal_command(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let command = text_param(params, "command")
        .ok_or_else(|| DomainStateError::bad_request("terminalCommand needs a command."))?;
    let provider = text_param(params, "provider");
    let cloud_provider = || {
        provider
            .clone()
            .filter(|provider| is_cloud_provider(provider))
            .ok_or_else(|| {
                DomainStateError::bad_request(
                    "Choose a cloud provider: hetzner, vercel, daytona, e2b or digitalocean.",
                )
            })
    };
    let (title, text) = match command.as_str() {
        "install" => (
            "Install agentbox".to_string(),
            "npm install -g @madarco/agentbox && agentbox install".to_string(),
        ),
        "setup" => {
            let provider = provider.clone().unwrap_or_else(|| "docker".to_string());
            if provider_spec(&provider).is_none() && provider != "remote-docker" {
                return Err(DomainStateError::bad_request(format!(
                    "\"{provider}\" is not an agentbox provider."
                )));
            }
            let label = if provider == "remote-docker" {
                "your own server".to_string()
            } else {
                provider_label(&provider)
            };
            (
                format!("Set up {label}"),
                format!("agentbox install -p {provider}"),
            )
        }
        "login" => {
            let provider = cloud_provider()?;
            (
                format!("Log in to {}", provider_label(&provider)),
                format!("agentbox {provider} login"),
            )
        }
        "prepare" => {
            let provider = provider
                .clone()
                .filter(|provider| provider_spec(provider).is_some())
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Choose a provider to prepare: docker, hetzner, vercel, daytona, e2b or digitalocean.",
                    )
                })?;
            (
                format!("Prepare {}", provider_label(&provider)),
                format!("agentbox prepare --provider {provider}"),
            )
        }
        "agentLogin" => {
            let agent = text_param(params, "agent")
                .filter(|agent| matches!(agent.as_str(), "claude" | "codex"))
                .ok_or_else(|| {
                    DomainStateError::bad_request("Choose claude or codex to sign in for boxes.")
                })?;
            let label = if agent == "claude" { "Claude" } else { "Codex" };
            (
                format!("Sign in to {label} for boxes"),
                format!("agentbox {agent} login"),
            )
        }
        "doctor" => ("Check agentbox".to_string(), "agentbox doctor".to_string()),
        "remoteDockerDoctor" => {
            let host = text_param(params, "host")
                .filter(|host| is_valid_ssh_target(host))
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Enter an SSH host such as myserver or user@host.example.com.",
                    )
                })?;
            (
                format!("Check {host}"),
                format!("agentbox remote-docker doctor {host}"),
            )
        }
        "remoteDockerAdd" => {
            let alias = text_param(params, "alias")
                .filter(|alias| is_valid_alias(alias))
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Name the server with letters, digits, dots, dashes or underscores.",
                    )
                })?;
            let ssh = text_param(params, "ssh")
                .filter(|ssh| is_valid_ssh_target(ssh))
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Enter an SSH host such as myserver or user@host.example.com:22.",
                    )
                })?;
            (
                format!("Add {alias}"),
                format!("agentbox remote-docker add {alias} {ssh}"),
            )
        }
        _ => {
            return Err(DomainStateError::bad_request(format!(
                "\"{command}\" is not an agentbox terminal command."
            )))
        }
    };
    Ok(json!({ "command": text, "title": title }))
}
