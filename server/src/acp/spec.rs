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
/// spaces on POSIX, PowerShell doubled quotes and statement separators, Windows CRT backslash
/// rules before double quotes, and preserves Windows paths with spaces).
pub(crate) fn tokenize_cli_command(command: &str) -> Vec<String> {
    tokenize_cli_command_for_platform(command, cfg!(windows))
}

fn tokenize_cli_command_for_platform(command: &str, is_windows: bool) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = command.trim().chars().peekable();

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
                        '\\' if is_windows => {
                            // Windows CRT CommandLineToArgvW rule: 2n backslashes before `"` emit n
                            // backslashes and close the quote; 2n+1 backslashes before `"` emit n
                            // backslashes and a literal `"`; backslashes not before `"` are literal.
                            let mut backslashes = 1usize;
                            while chars.peek() == Some(&'\\') {
                                chars.next();
                                backslashes += 1;
                            }
                            if chars.peek() == Some(&'"') {
                                for _ in 0..(backslashes / 2) {
                                    current.push('\\');
                                }
                                if backslashes % 2 == 1 {
                                    chars.next();
                                    current.push('"');
                                }
                            } else {
                                for _ in 0..backslashes {
                                    current.push('\\');
                                }
                            }
                        }
                        '\\' => match chars.peek().copied() {
                            Some(esc @ ('"' | '\\' | '$' | '`')) => {
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
            ';' if is_windows => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
                tokens.push(";".to_string());
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

fn is_valid_env_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn strip_ps_env_prefix(token: &str) -> Option<&str> {
    if token.len() > 5 && token[..5].eq_ignore_ascii_case("$env:") {
        Some(&token[5..])
    } else {
        None
    }
}

fn split_leading_env_assignments(tokens: Vec<String>) -> (Vec<(String, String)>, Vec<String>) {
    let mut envs = Vec::new();
    let mut idx = 0;
    let mut saw_env_cmd = false;

    while idx < tokens.len() {
        let token = &tokens[idx];

        if token == ";" {
            idx += 1;
            continue;
        }

        // PowerShell `$env:KEY=VAL` (with optional trailing `;` or spaces around `=`).
        if let Some(rest) = strip_ps_env_prefix(token) {
            if let Some((k, v)) = rest.split_once('=') {
                if is_valid_env_name(k) {
                    let val = v.strip_suffix(';').unwrap_or(v);
                    envs.push((k.to_string(), val.to_string()));
                    idx += 1;
                    continue;
                }
            } else if is_valid_env_name(rest) && idx + 1 < tokens.len() {
                if tokens[idx + 1] == "=" && idx + 2 < tokens.len() {
                    let val = tokens[idx + 2]
                        .strip_suffix(';')
                        .unwrap_or(&tokens[idx + 2]);
                    envs.push((rest.to_string(), val.to_string()));
                    idx += 3;
                    continue;
                }
                if let Some(after_eq) = tokens[idx + 1].strip_prefix('=') {
                    let val = after_eq.strip_suffix(';').unwrap_or(after_eq);
                    envs.push((rest.to_string(), val.to_string()));
                    idx += 2;
                    continue;
                }
            }
            break;
        }

        // POSIX `KEY=VAL` assignment.
        if let Some((k, v)) = token.split_once('=') {
            if is_valid_env_name(k) {
                envs.push((k.to_string(), v.to_string()));
                idx += 1;
                continue;
            }
        }

        // POSIX `env` / `/usr/bin/env` wrapper (with optional `--`) when followed by a target.
        if !saw_env_cmd && (token == "env" || token == "/usr/bin/env") && idx + 1 < tokens.len() {
            saw_env_cmd = true;
            idx += 1;
            if idx < tokens.len() && tokens[idx] == "--" && idx + 1 < tokens.len() {
                idx += 1;
            }
            continue;
        }

        break;
    }

    // Drop a PowerShell call operator `&` that followed leading `$env:` assignments.
    if idx + 1 < tokens.len() && tokens[idx] == "&" {
        idx += 1;
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
    let (envs, mut base_tokens) = cli_command
        .map(tokenize_cli_command)
        .map(split_leading_env_assignments)
        .unwrap_or_default();
    if base_tokens.is_empty() {
        base_tokens.push(spec.default_cli_bin.to_string());
    }
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
