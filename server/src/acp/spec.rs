//! Per-agent ACP adapter specs. Adding another ACP-capable agent (such as Codex)
//! only requires registering its spec here.

use std::path::PathBuf;

/// Static capabilities and CLI bridge metadata for one ACP-capable agent family.
#[derive(Clone, Copy, Debug)]
pub struct AcpAgentSpec {
    pub family: &'static str,
    pub display_name: &'static str,
    /// Default command + arguments to start the agent's stdio ACP server.
    pub acp_argv: &'static [&'static str],
    /// Executable name used when switching in-place to the interactive CLI terminal.
    pub default_cli_bin: &'static str,
    /// Flag passed to the interactive CLI to resume the conversation id created over ACP.
    pub cli_resume_flag: &'static str,
    /// Optional flag added to the interactive CLI when accept-all permissions are enabled.
    pub cli_accept_all_flag: Option<&'static str>,
}

const ANTIGRAVITY_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "antigravity",
    display_name: "Antigravity CLI",
    acp_argv: &["agy", "acp", "--stdio"],
    default_cli_bin: "agy",
    cli_resume_flag: "--conversation",
    cli_accept_all_flag: Some("--dangerously-skip-permissions"),
};

pub fn spec_for_family(family: &str) -> Option<&'static AcpAgentSpec> {
    match family.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "antigravity-cli" | "agy" => Some(&ANTIGRAVITY_SPEC),
        _ => None,
    }
}

/// Resolve the local plain-text transcript path for an ACP session when the agent
/// writes one alongside its encrypted/internal conversation store.
pub fn transcript_path_for_session(family: &str, native_session_id: &str) -> Option<PathBuf> {
    if native_session_id.trim().is_empty() {
        return None;
    }
    match family {
        "antigravity" => {
            if !crate::session_chat_antigravity_mirror::is_safe_antigravity_session_id(
                native_session_id,
            ) {
                return None;
            }
            let logs = crate::session_chat_antigravity_mirror::antigravity_app_data_dir()
                .join("brain")
                .join(native_session_id)
                .join(".system_generated")
                .join("logs");
            let full = logs.join("transcript_full.jsonl");
            if full.is_file() {
                return Some(full);
            }
            let plain = logs.join("transcript.jsonl");
            if plain.is_file() {
                return Some(plain);
            }
            // Return the expected path so the follower binds as soon as the first step lands.
            Some(full)
        }
        _ => None,
    }
}

fn shell_single_quote(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'_' | b'-' | b'.' | b'/' | b':' | b'=' | b'@' | b'+')
        })
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Build the command that `zmx` runs inside the session PTY to start `ghostex acp-host`.
pub fn build_acp_host_launch_command(
    family: &str,
    cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    accept_all: bool,
    model: Option<&str>,
    effort: Option<&str>,
) -> String {
    let mut parts = vec![
        "\"${GHOSTEX_CLI_EXECUTABLE:-ghostex}\"".to_string(),
        "acp-host".to_string(),
        "--agent".to_string(),
        shell_single_quote(family),
    ];
    if let Some(cmd) = cli_command.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--cli-command".to_string());
        parts.push(shell_single_quote(cmd));
    }
    if let Some(id) = resume_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--resume".to_string());
        parts.push(shell_single_quote(id));
    }
    if accept_all {
        parts.push("--accept-all".to_string());
    }
    if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--model".to_string());
        parts.push(shell_single_quote(m));
    }
    if let Some(e) = effort.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--effort".to_string());
        parts.push(shell_single_quote(e));
    }
    parts.join(" ")
}

/// Build the argv to exec in-place inside `zmx` when the user switches an ACP session
/// to the interactive CLI terminal.
pub fn build_cli_switch_argv(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
    native_session_id: Option<&str>,
    accept_all: bool,
) -> (String, Vec<String>) {
    let base_tokens: Vec<String> = cli_command
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .filter(|tokens: &Vec<String>| !tokens.is_empty())
        .unwrap_or_else(|| vec![spec.default_cli_bin.to_string()]);
    let program = base_tokens[0].clone();
    let mut args = base_tokens[1..].to_vec();
    if accept_all {
        if let Some(flag) = spec.cli_accept_all_flag {
            if !args.iter().any(|arg| arg == flag) {
                args.push(flag.to_string());
            }
        }
    }
    if let Some(id) = native_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        if !args.iter().any(|arg| arg == spec.cli_resume_flag) {
            args.push(spec.cli_resume_flag.to_string());
            args.push(id.to_string());
        }
    }
    (program, args)
}
