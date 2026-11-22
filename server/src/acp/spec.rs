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

#[cfg_attr(windows, allow(dead_code))]
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

fn quote_acp_launch_arg(value: &str, windows: bool) -> String {
    if windows {
        crate::platform::shell::powershell_native_arg(value)
    } else {
        shell_single_quote(value)
    }
}

/// Build the command that `zmx`/`wmx` runs inside the session PTY to start `ghostex acp-host`.
pub fn build_acp_host_launch_command(
    family: &str,
    cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    accept_all: bool,
    model: Option<&str>,
    effort: Option<&str>,
) -> String {
    build_acp_host_launch_command_for_platform(
        family,
        cli_command,
        resume_session_id,
        accept_all,
        model,
        effort,
        cfg!(windows),
    )
}

fn build_acp_host_launch_command_for_platform(
    family: &str,
    cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    accept_all: bool,
    model: Option<&str>,
    effort: Option<&str>,
    windows: bool,
) -> String {
    let mut parts = if windows {
        vec![
            "& $(if ($env:GHOSTEX_CLI_EXECUTABLE) { $env:GHOSTEX_CLI_EXECUTABLE } else { 'ghostex' })"
                .to_string(),
            "acp-host".to_string(),
            "--agent".to_string(),
            quote_acp_launch_arg(family, true),
        ]
    } else {
        vec![
            "\"${GHOSTEX_CLI_EXECUTABLE:-ghostex}\"".to_string(),
            "acp-host".to_string(),
            "--agent".to_string(),
            quote_acp_launch_arg(family, false),
        ]
    };
    if let Some(cmd) = cli_command.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--cli-command".to_string());
        parts.push(quote_acp_launch_arg(cmd, windows));
    }
    if let Some(id) = resume_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--resume".to_string());
        parts.push(quote_acp_launch_arg(id, windows));
    }
    if accept_all {
        parts.push("--accept-all".to_string());
    }
    if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--model".to_string());
        parts.push(quote_acp_launch_arg(m, windows));
    }
    if let Some(e) = effort.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--effort".to_string());
        parts.push(quote_acp_launch_arg(e, windows));
    }
    parts.join(" ")
}

/// Shell-aware tokenizer for a stored CLI command line (handles single/double quotes, escaped
/// spaces on POSIX, PowerShell doubled single quotes, and preserves Windows paths with spaces).
pub(crate) fn tokenize_cli_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = command.trim().chars().peekable();
    let is_windows = cfg!(windows);

    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                in_token = true;
                while let Some(inner) = chars.next() {
                    if inner == '\'' {
                        // PowerShell escapes a literal single quote inside '...' by doubling it (`''`),
                        // whereas POSIX closes and reopens; if the next char is also `'` and the
                        // token looks PowerShell-quoted (on Windows), emit one `'`.
                        if is_windows && chars.peek() == Some(&'\'') {
                            chars.next();
                            current.push('\'');
                        } else {
                            break;
                        }
                    } else {
                        current.push(inner);
                    }
                }
            }
            '"' => {
                in_token = true;
                while let Some(inner) = chars.next() {
                    match inner {
                        '"' => {
                            if is_windows && chars.peek() == Some(&'"') {
                                chars.next();
                                current.push('"');
                            } else {
                                break;
                            }
                        }
                        '\\' => match chars.peek().copied() {
                            Some('"') => {
                                chars.next();
                                current.push('"');
                            }
                            Some('\\') if !is_windows => {
                                chars.next();
                                current.push('\\');
                            }
                            Some(esc @ ('$' | '`')) if !is_windows => {
                                chars.next();
                                current.push(esc);
                            }
                            _ => current.push('\\'),
                        },
                        other => current.push(other),
                    }
                }
            }
            '\\' if !is_windows => {
                in_token = true;
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                } else {
                    current.push('\\');
                }
            }
            ch if ch.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            other => {
                in_token = true;
                current.push(other);
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    // Drop a leading PowerShell call operator (`& 'C:\Program Files\...\agy.exe'`).
    if tokens.len() >= 2 && tokens[0] == "&" {
        tokens.remove(0);
    }
    tokens
}

fn split_leading_env_assignments(tokens: Vec<String>) -> (Vec<(String, String)>, Vec<String>) {
    let mut envs = Vec::new();
    let mut idx = 0;
    while idx < tokens.len() {
        let token = &tokens[idx];
        let is_env = token
            .split_once('=')
            .filter(|(k, _)| {
                !k.is_empty()
                    && k.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .map(|(k, v)| (k.to_string(), v.to_string()));
        if let Some(pair) = is_env {
            envs.push(pair);
            idx += 1;
        } else {
            break;
        }
    }
    (envs, tokens[idx..].to_vec())
}

/// Build `(envs, program, args)` to launch the agent's stdio ACP subprocess, respecting any custom
/// executable configured via `cli_command`.
pub fn build_acp_server_argv(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
) -> Option<(Vec<(String, String)>, String, Vec<String>)> {
    let (default_prog, acp_args) = spec.acp_argv.split_first()?;
    let (envs, base_tokens) = cli_command
        .map(tokenize_cli_command)
        .map(split_leading_env_assignments)
        .unwrap_or_default();
    let program = base_tokens
        .first()
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| (*default_prog).to_string());
    let args = acp_args.iter().map(|s| (*s).to_string()).collect();
    Some((envs, program, args))
}

/// Build `(envs, program, args)` to exec in-place inside `zmx`/`wmx` when the user switches an ACP
/// session to the interactive CLI terminal.
pub fn build_cli_switch_argv(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
    native_session_id: Option<&str>,
    accept_all: bool,
) -> (Vec<(String, String)>, String, Vec<String>) {
    let (envs, base_tokens) = cli_command
        .map(tokenize_cli_command)
        .map(split_leading_env_assignments)
        .filter(|(_, tokens)| !tokens.is_empty())
        .unwrap_or_else(|| (Vec::new(), vec![spec.default_cli_bin.to_string()]));
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
    (envs, program, args)
}
