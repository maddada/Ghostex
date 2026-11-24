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
    /// Whether the Native protocol supports sending a follow-up user message while a turn is
    /// already running without cancelling the active turn.
    pub supports_mid_turn_prompt: bool,
}

const ANTIGRAVITY_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "antigravity",
    display_name: "Antigravity",
    // `--cli_dir_name ghostex-acp` keeps `agy acp` on its default `request-review` policy
    // (isolated from any `toolPermission: always-proceed` in the user's CLI settings.json) while
    // sharing the user's existing login and `~/.gemini/antigravity-cli/brain/` transcripts so
    // Ghostex's four live approval levels (Ask, Auto-edits, Auto, Full access) are authoritative.
    acp_argv: &["agy", "acp", "--stdio", "--cli_dir_name", "ghostex-acp"],
    default_cli_bin: "agy",
    cli_resume_flag: "--conversation",
    cli_accept_all_flag: Some("--dangerously-skip-permissions"),
    supports_mid_turn_prompt: true,
};

const CLAUDE_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "claude",
    display_name: "Claude Code",
    // Native bidirectional stream-json control protocol with stdio permission routing.
    // Always starts in `--permission-mode default` so Ghostex's four live approval levels are
    // authoritative and `AskUserQuestion` / `ExitPlanMode` always prompt interactively.
    acp_argv: &[
        "claude",
        "--output-format",
        "stream-json",
        "--verbose",
        "--input-format",
        "stream-json",
        "--permission-prompt-tool=stdio",
        "--include-partial-messages",
        "--permission-mode",
        "default",
    ],
    default_cli_bin: "claude",
    cli_resume_flag: "--resume",
    cli_accept_all_flag: Some("--dangerously-skip-permissions"),
    supports_mid_turn_prompt: true,
};

const CODEX_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "codex",
    display_name: "Codex",
    // Native Codex app-server JSON-RPC over stdio.
    acp_argv: &[
        "codex",
        "app-server",
        "--stdio",
        "-c",
        "thread_unload_delay_secs=0",
    ],
    default_cli_bin: "codex",
    cli_resume_flag: "resume",
    cli_accept_all_flag: Some("--dangerously-bypass-approvals-and-sandbox"),
    supports_mid_turn_prompt: true,
};

pub fn spec_for_family(family: &str) -> Option<&'static AcpAgentSpec> {
    match family.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "antigravity-cli" | "agy" => Some(&ANTIGRAVITY_SPEC),
        "claude" | "claude-code" => Some(&CLAUDE_SPEC),
        "codex" | "codex-cli" => Some(&CODEX_SPEC),
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
        "claude" => crate::session_chat_paths::resolve_session_chat_transcript_path(
            crate::session_chat::SessionChatTranscriptAgent::Claude,
            Some(native_session_id),
            None,
        ),
        "codex" => crate::session_chat_paths::resolve_session_chat_transcript_path(
            crate::session_chat::SessionChatTranscriptAgent::Codex,
            Some(native_session_id),
            None,
        ),
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

/// Validated `cswap` / `xswap` account wrapper extracted from Ghostex's account launch command
/// (never carrying arbitrary flags or leading environment assignments).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpAccountWrapper {
    pub bin: String,
    pub slot: String,
}

/// Server-generated structured launch profile for `ghostex acp-host`. Non-CLI launches build the
/// agent subprocess argv exclusively from these Ghostex-owned fields and Ghostex's own settings,
/// NEVER forwarding flags or leading env vars from a user- or agent-supplied command string.
#[derive(Clone, Debug, Default)]
pub struct AcpLaunchProfile {
    pub account_wrapper: Option<AcpAccountWrapper>,
    pub hub_id: Option<String>,
    pub connector_envs: Vec<(String, String)>,
    pub append_system_prompt_file: Option<String>,
    pub developer_instructions: Option<String>,
    pub gbot_chat: bool,
    pub gbot_api_chat: bool,
    pub gbot_computer_use_off: bool,
    pub lean_worker: bool,
}

/// Build the `ghostex acp-host` launch command with Ghostex's server-generated structured profile.
///
/// CDXC:AgentProviders 2026-11-24 WHY: `_cli_command` (the session's terminal-mode command) is no
/// longer written as `--cli-command`. acp-host has ignored its value since the ACP account-wrapper
/// fix (`build_acp_server_argv` and `build_cli_switch_argv_with_options` take `_cli_command`), so
/// it only repeated `--model`/`--effort` and carried the terminal's `--dangerously-skip-permissions`
/// into an Ask session's stored command, where `command_has_accept_all_flag` read the session as
/// having run with accept-all (restore and launch-settings gates refused it from other devices).
/// The approval level a Native session runs at is `--approval-level` alone.
#[allow(clippy::too_many_arguments)]
pub fn build_acp_host_launch_command_with_profile(
    family: &str,
    _cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    approval_level: super::AcpApprovalLevel,
    writable_roots: &[String],
    model: Option<&str>,
    effort: Option<&str>,
    fork_session: bool,
    read_only: bool,
    profile: &AcpLaunchProfile,
) -> String {
    build_acp_host_launch_command_for_platform(
        family,
        resume_session_id,
        approval_level,
        writable_roots,
        model,
        effort,
        fork_session,
        read_only,
        profile,
        cfg!(windows),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_acp_host_launch_command_for_platform(
    family: &str,
    resume_session_id: Option<&str>,
    approval_level: super::AcpApprovalLevel,
    writable_roots: &[String],
    model: Option<&str>,
    effort: Option<&str>,
    fork_session: bool,
    read_only: bool,
    profile: &AcpLaunchProfile,
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
    if let Some(id) = resume_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--resume".to_string());
        parts.push(quote_acp_launch_arg(id, windows));
        if fork_session {
            parts.push("--fork-session".to_string());
        }
    }
    if read_only {
        parts.push("--read-only".to_string());
    }
    parts.push("--approval-level".to_string());
    parts.push(quote_acp_launch_arg(approval_level.as_str(), windows));
    for root in writable_roots
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push("--writable-root".to_string());
        parts.push(quote_acp_launch_arg(root, windows));
    }
    if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--model".to_string());
        parts.push(quote_acp_launch_arg(m, windows));
    }
    if let Some(e) = effort.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push("--effort".to_string());
        parts.push(quote_acp_launch_arg(e, windows));
    }
    if let Some(wrapper) = profile.account_wrapper.as_ref() {
        if is_valid_account_wrapper(family, wrapper) {
            parts.push("--account-wrapper-slot".to_string());
            parts.push(quote_acp_launch_arg(&wrapper.slot, windows));
        }
    }
    if let Some(hub_id) = profile
        .hub_id
        .as_deref()
        .map(str::trim)
        .filter(|id| is_safe_hub_id(id))
    {
        parts.push("--hub-id".to_string());
        parts.push(quote_acp_launch_arg(hub_id, windows));
    }
    for (k, v) in &profile.connector_envs {
        if is_allowed_connector_env_key(k) {
            parts.push("--connector-env".to_string());
            parts.push(quote_acp_launch_arg(&format!("{k}={v}"), windows));
        }
    }
    if let Some(path) = profile
        .append_system_prompt_file
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push("--append-system-prompt-file".to_string());
        parts.push(quote_acp_launch_arg(path, windows));
    }
    if let Some(instr) = profile
        .developer_instructions
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // Keep the `ghostex acp-host` launch line strictly single-line (no control chars like
        // `\n`) so account switches (`account_switch_resume_command`) and raw shell writes never
        // reject or split multiline Codex developer instructions.
        if instr.chars().any(char::is_control) {
            if let Ok(encoded) = serde_json::to_string(instr) {
                parts.push("--developer-instructions-json".to_string());
                parts.push(quote_acp_launch_arg(&encoded, windows));
            }
        } else {
            parts.push("--developer-instructions".to_string());
            parts.push(quote_acp_launch_arg(instr, windows));
        }
    }
    if profile.gbot_chat {
        parts.push("--gbot-chat".to_string());
    }
    if profile.gbot_api_chat {
        parts.push("--gbot-api-chat".to_string());
    }
    if profile.gbot_computer_use_off {
        parts.push("--gbot-computer-use-off".to_string());
    }
    if profile.lean_worker {
        parts.push("--lean-worker".to_string());
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
    if token
        .as_bytes()
        .get(..5)
        .is_some_and(|head| head.eq_ignore_ascii_case(b"$env:"))
    {
        token.get(5..).filter(|rest| !rest.is_empty())
    } else {
        None
    }
}

pub(crate) fn split_leading_env_assignments(
    tokens: Vec<String>,
) -> (Vec<(String, String)>, Vec<String>) {
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

pub(crate) fn is_safe_hub_id(id: &str) -> bool {
    let trimmed = id.trim();
    !trimmed.is_empty()
        && !trimmed.contains("..")
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub(crate) fn is_allowed_connector_env_key(key: &str) -> bool {
    matches!(
        key,
        "GH_CONFIG_DIR"
            | "GOOGLE_WORKSPACE_CLI_CONFIG_DIR"
            | "GWS_CONFIG_DIR"
            | "GWS_CREDENTIALS_FILE"
    ) || key.starts_with("GH_CONFIG_")
        || key.starts_with("GWS_")
        || key.starts_with("GOOGLE_WORKSPACE_")
}

pub(crate) fn is_valid_account_wrapper_slot(slot: &str) -> bool {
    !slot.is_empty()
        && slot
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && !slot.contains("..")
        && slot
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Resolve Ghostex's installed `cswap` or `xswap` executable strictly from Ghostex's own accounts
/// helper (never from a caller-supplied command path). Falls back to the bare helper name on PATH.
pub(crate) fn resolve_account_wrapper_executable(family: &str) -> Option<String> {
    let helper = match family {
        "claude" => "cswap",
        "codex" => "xswap",
        _ => return None,
    };
    let resolved = crate::accounts::launch::home()
        .ok()
        .and_then(|home| crate::accounts::helpers::executable(&home, helper))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| helper.to_string());
    Some(resolved)
}

pub(crate) fn is_valid_account_wrapper(family: &str, wrapper: &AcpAccountWrapper) -> bool {
    matches!(family, "claude" | "codex") && is_valid_account_wrapper_slot(&wrapper.slot)
}

/// Derive a validated `AcpAccountWrapper` strictly from Ghostex's own server-side account state
/// (`account_command` produced by `accounts::launch::apply_new_session` / `runtimeSettings.accountCommand`,
/// or `runtimeSettings.accountSlot`) and resolve the helper executable through `accounts::helpers::executable`.
/// NEVER parses an untrusted `agentCommand` or `cli_command`.
pub(crate) fn account_wrapper_from_ghostex_account(
    family: &str,
    runtime_settings: Option<&serde_json::Map<String, serde_json::Value>>,
    account_command: Option<&str>,
) -> Option<AcpAccountWrapper> {
    let expected_helper = match family {
        "claude" => "cswap",
        "codex" => "xswap",
        _ => return None,
    };
    let slot_from_cmd = account_command
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|cmd| {
            let tokens = tokenize_cli_command(cmd);
            let (_, base_tokens) = split_leading_env_assignments(tokens);
            let first = base_tokens.first()?;
            let base = first
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(first)
                .to_ascii_lowercase();
            let matches_helper =
                base == expected_helper || base == format!("{expected_helper}.exe");
            if !matches_helper || base_tokens.get(1).map(String::as_str) != Some("run") {
                return None;
            }
            let slot = base_tokens.get(2)?.trim();
            is_valid_account_wrapper_slot(slot).then(|| slot.to_string())
        });
    let slot = slot_from_cmd.or_else(|| {
        runtime_settings
            .and_then(|rt| rt.get("accountSlot"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| is_valid_account_wrapper_slot(s))
            .map(str::to_string)
    })?;
    let bin = resolve_account_wrapper_executable(family)?;
    Some(AcpAccountWrapper { bin, slot })
}

fn is_safe_model_or_effort_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('-')
        && !trimmed.contains("..")
        && trimmed.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | ':' | '/' | '@' | '[' | ']' | '=' | '+')
        })
}

/// Extract safe `--model` / `-m` and `--effort` values from an owner-configured agent command
/// (used only as a fallback when no explicit create param or remembered model pin is present).
pub(crate) fn extract_safe_model_and_effort_from_command(
    family: &str,
    cmd: &str,
) -> (Option<String>, Option<String>) {
    if !matches!(family, "claude" | "codex" | "antigravity") {
        return (None, None);
    }
    let tokens = tokenize_cli_command(cmd);
    let (_, base_tokens) = split_leading_env_assignments(tokens);
    let mut model = None;
    let mut effort = None;
    let mut i = 1;
    while i < base_tokens.len() {
        let tok = base_tokens[i].as_str();
        if let Some(v) = tok
            .strip_prefix("--model=")
            .or_else(|| tok.strip_prefix("-m="))
        {
            if model.is_none() && is_safe_model_or_effort_token(v) {
                model = Some(v.trim().to_string());
            }
        } else if matches!(tok, "--model" | "-m") {
            if let Some(next) = base_tokens.get(i + 1) {
                if model.is_none() && is_safe_model_or_effort_token(next) {
                    model = Some(next.trim().to_string());
                }
                i += 1;
            }
        } else if let Some(v) = tok.strip_prefix("--effort=") {
            if effort.is_none() && is_safe_model_or_effort_token(v) {
                effort = Some(v.trim().to_string());
            }
        } else if tok == "--effort" {
            if let Some(next) = base_tokens.get(i + 1) {
                if effort.is_none() && is_safe_model_or_effort_token(next) {
                    effort = Some(next.trim().to_string());
                }
                i += 1;
            }
        }
        i += 1;
    }
    (model, effort)
}

/// Inspect an owner-configured raw agent command (`agent_config.command` / `launchSettings.agentCommand`
/// BEFORE Ghostex appends its own wrappers or flags) and return any leading env vars, custom binary
/// paths/wrappers, or extra flags that Non-CLI mode does not forward (excluding the bare default
/// family CLI and honored `--model` / `-m` / `--effort`).
pub(crate) fn ignored_owner_command_tokens(
    family: &str,
    owner_command: Option<&str>,
) -> Vec<String> {
    let default_cli_bin = match family {
        "claude" => "claude",
        "codex" => "codex",
        "antigravity" => "agy",
        _ => return Vec::new(),
    };
    let Some(raw) = owner_command.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let tokens = tokenize_cli_command(raw);
    let (envs, base_tokens) = split_leading_env_assignments(tokens);
    let mut ignored: Vec<String> = envs.into_iter().map(|(k, _)| format!("{k}=...")).collect();
    let Some(first) = base_tokens.first() else {
        return ignored;
    };
    // Plain bare CLI binary (or `command claude`) is normal; a path-qualified or custom wrapper
    // binary is ignored in Non-CLI mode.
    let (cmd_tok, rest_start) = if first == "command" && base_tokens.len() > 1 {
        (base_tokens[1].as_str(), 2usize)
    } else {
        (first.as_str(), 1usize)
    };
    let lower_cmd = cmd_tok.to_ascii_lowercase();
    let is_plain_default = !cmd_tok.contains('/')
        && !cmd_tok.contains('\\')
        && (lower_cmd == default_cli_bin || lower_cmd == format!("{default_cli_bin}.exe"));
    if !is_plain_default {
        ignored.push(cmd_tok.to_string());
    }
    let mut i = rest_start;
    while i < base_tokens.len() {
        let tok = base_tokens[i].as_str();
        if let Some(v) = tok
            .strip_prefix("--model=")
            .or_else(|| tok.strip_prefix("-m="))
            .or_else(|| tok.strip_prefix("--effort="))
        {
            if !is_safe_model_or_effort_token(v) {
                ignored.push(tok.to_string());
            }
        } else if matches!(tok, "--model" | "-m" | "--effort") {
            if let Some(next) = base_tokens.get(i + 1) {
                if !is_safe_model_or_effort_token(next) {
                    ignored.push(tok.to_string());
                    ignored.push(next.clone());
                }
                i += 1;
            } else {
                ignored.push(tok.to_string());
            }
        } else {
            ignored.push(tok.to_string());
        }
        i += 1;
    }
    ignored
}

/// Resolve Ghostex's own CLIProxyAPI hub profile directory (`~/.claude-profiles/ghostex-<id>` or
/// `~/.codex-profiles/ghostex-<id>`) for a Non-CLI session. Returns `None` if `hub_id` is invalid.
pub(crate) fn resolve_hub_profile_dir(family: &str, hub_id: &str) -> Option<PathBuf> {
    if !is_safe_hub_id(hub_id) {
        return None;
    }
    let provider = match family {
        "claude" => crate::accounts::model::Provider::Claude,
        "codex" => crate::accounts::model::Provider::Codex,
        _ => return None,
    };
    let home = crate::accounts::launch::home().ok()?;
    Some(crate::accounts::hub::launch::profile_dir(
        &home,
        provider,
        hub_id.trim(),
    ))
}

/// Build `(envs, program, args)` to launch the agent's stdio ACP/SDK subprocess strictly from
/// Ghostex's static per-agent spec and server-generated `AcpLaunchProfile`.
///
/// NEVER forwards arbitrary flags, leading environment assignments, custom binary paths, or account
/// wrappers parsed from a user- or agent-supplied command string (`cli_command`).
pub fn build_acp_server_argv(
    spec: &AcpAgentSpec,
    _cli_command: Option<&str>,
    profile: &AcpLaunchProfile,
) -> Option<(Vec<(String, String)>, String, Vec<String>)> {
    let (default_prog, acp_args) = spec.acp_argv.split_first()?;

    let wrapper = profile
        .account_wrapper
        .as_ref()
        .filter(|w| is_valid_account_wrapper(spec.family, w))
        .and_then(|w| {
            let bin = resolve_account_wrapper_executable(spec.family)?;
            Some(AcpAccountWrapper {
                bin,
                slot: w.slot.clone(),
            })
        });

    let mut envs: Vec<(String, String)> = Vec::new();
    if wrapper.is_none() {
        if let Some(hub_id) = profile.hub_id.as_deref() {
            if let Some(dir) = resolve_hub_profile_dir(spec.family, hub_id) {
                let env_key = match spec.family {
                    "claude" => Some("CLAUDE_CONFIG_DIR"),
                    "codex" => Some("CODEX_HOME"),
                    _ => None,
                };
                if let Some(key) = env_key {
                    envs.push((key.to_string(), dir.to_string_lossy().to_string()));
                }
            }
        }
    }

    if let Some(wrapper) = wrapper {
        let mut args = vec!["run".to_string(), wrapper.slot];
        if spec.family == "claude" {
            args.push("--share-history".to_string());
        }
        args.push("--".to_string());
        args.extend(acp_args.iter().map(|s| (*s).to_string()));
        return Some((envs, wrapper.bin, args));
    }

    let program = (*default_prog).to_string();
    let args: Vec<String> = acp_args.iter().map(|s| (*s).to_string()).collect();
    Some((envs, program, args))
}

/// Build Ghostex's own server-side Claude `--settings` JSON strictly from Ghostex's structured
/// profile and Ghostex settings (never from any caller-supplied `--settings`, never containing
/// `hooks`, command helpers, `permissions.allow`, `permissions.defaultMode`, or
/// `permissions.additionalDirectories`).
pub(crate) fn build_ghostex_claude_settings_json(
    profile: &AcpLaunchProfile,
    cwd: Option<&str>,
    effort: Option<&str>,
    skip_dangerous_prompt: bool,
) -> Option<String> {
    let mut settings = serde_json::Map::new();
    let mut env_map = serde_json::Map::new();
    for (k, v) in &profile.connector_envs {
        if is_allowed_connector_env_key(k) {
            env_map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }
    }
    if !env_map.is_empty() {
        settings.insert("env".to_string(), serde_json::Value::Object(env_map));
    }
    crate::agents::write_compaction_into_settings(&mut settings);
    if effort
        .map(str::trim)
        .is_some_and(|e| matches!(e, "low" | "medium" | "high" | "xhigh" | "max"))
    {
        settings.insert("ultracode".to_string(), serde_json::Value::Bool(false));
    }
    if skip_dangerous_prompt {
        settings.insert(
            "skipDangerousModePermissionPrompt".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    crate::agent_skills_session::write_ghostex_skills_into_settings(&mut settings);

    let mut deny_rules = crate::agents::claude_deny_rules_for_session(cwd, profile.gbot_api_chat);
    if profile.gbot_computer_use_off {
        for rule in crate::agents::claude_screen_deny_rules() {
            if !deny_rules.contains(&rule) {
                deny_rules.push(rule);
            }
        }
    }
    if !deny_rules.is_empty() {
        let mut perms = serde_json::Map::new();
        perms.insert("deny".to_string(), serde_json::Value::Array(deny_rules));
        settings.insert("permissions".to_string(), serde_json::Value::Object(perms));
    }
    if settings.is_empty() {
        None
    } else {
        serde_json::to_string(&serde_json::Value::Object(settings)).ok()
    }
}

/// Accept an `--append-system-prompt-file` path ONLY when it is a regular file living inside
/// Ghostex's own root/state directory (`GxserverPaths::root_dir`).
pub(crate) fn validated_ghostex_prompt_file(raw: Option<&str>) -> Option<String> {
    let candidate = PathBuf::from(raw?.trim());
    if !candidate.is_file() {
        return None;
    }
    let root = crate::paths::get_gxserver_paths(None).root_dir;
    let canon_root = std::fs::canonicalize(&root).ok()?;
    let canon_file = std::fs::canonicalize(&candidate).ok()?;
    if canon_file.starts_with(&canon_root) {
        Some(canon_file.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Build `(envs, program, args)` to exec in-place inside `zmx`/`wmx` when the user switches an ACP
/// session to the interactive CLI terminal. Builds the interactive CLI strictly from Ghostex's
/// structured `AcpLaunchProfile`, preserving `--permission-mode plan` /
/// `--sandbox read-only --ask-for-approval on-request` for `--read-only` threads and re-applying
/// Ghostex's own deny profiles cleanly for the session's current approval level (never leaking
/// untrusted command flags, arbitrary binary paths, or a full-write `ghostex-home` profile on
/// downgrade).
pub fn build_cli_switch_argv_with_options(
    spec: &AcpAgentSpec,
    _cli_command: Option<&str>,
    native_session_id: Option<&str>,
    accept_all: bool,
    read_only: bool,
    profile: &AcpLaunchProfile,
    cwd: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> (Vec<(String, String)>, String, Vec<String>) {
    let model = model.map(str::trim).filter(|m| {
        !m.is_empty() && !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
    });
    let effort = effort.map(str::trim).filter(|e| !e.is_empty());
    let wrapper = profile
        .account_wrapper
        .as_ref()
        .filter(|w| is_valid_account_wrapper(spec.family, w))
        .and_then(|w| {
            let bin = resolve_account_wrapper_executable(spec.family)?;
            Some(AcpAccountWrapper {
                bin,
                slot: w.slot.clone(),
            })
        });

    let mut envs: Vec<(String, String)> = Vec::new();
    if wrapper.is_none() {
        if let Some(hub_id) = profile.hub_id.as_deref() {
            if let Some(dir) = resolve_hub_profile_dir(spec.family, hub_id) {
                let env_key = match spec.family {
                    "claude" => Some("CLAUDE_CONFIG_DIR"),
                    "codex" => Some("CODEX_HOME"),
                    _ => None,
                };
                if let Some(key) = env_key {
                    envs.push((key.to_string(), dir.to_string_lossy().to_string()));
                }
            }
        }
    }

    let mut inner_args: Vec<String> = Vec::new();
    if let Some(id) = native_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        inner_args.push(spec.cli_resume_flag.to_string());
        inner_args.push(id.to_string());
    }

    if spec.family == "claude" {
        if let Some(m) = model {
            inner_args.push("--model".to_string());
            inner_args.push(m.to_string());
        }
        if let Some(e) = effort {
            inner_args.push("--effort".to_string());
            inner_args.push(e.to_string());
        }
        if let Some(settings_json) =
            build_ghostex_claude_settings_json(profile, cwd, effort, accept_all && !read_only)
        {
            inner_args.push("--settings".to_string());
            inner_args.push(settings_json);
        }
        if let Some(path) =
            validated_ghostex_prompt_file(profile.append_system_prompt_file.as_deref())
        {
            inner_args.push("--append-system-prompt-file".to_string());
            inner_args.push(path);
        }
    } else if spec.family == "codex" {
        if let Some(m) = model {
            inner_args.push("-m".to_string());
            inner_args.push(m.to_string());
        }
        if let Some(e) = effort {
            inner_args.push("-c".to_string());
            inner_args.push(format!(
                "model_reasoning_effort={}",
                crate::agents::codex_toml_string(e)
            ));
        }
        for (k, v) in &profile.connector_envs {
            if is_allowed_connector_env_key(k) {
                inner_args.push("-c".to_string());
                inner_args.push(format!(
                    "shell_environment_policy.set.{k}={}",
                    serde_json::to_string(v).unwrap_or_default()
                ));
            }
        }
        if profile.gbot_computer_use_off {
            let codex_home = envs
                .iter()
                .find(|(k, _)| k == "CODEX_HOME")
                .map(|(_, v)| PathBuf::from(v));
            for srv in crate::agents::codex_disabled_screen_servers(codex_home.as_deref()) {
                inner_args.push("-c".to_string());
                inner_args.push(format!("mcp_servers.{srv}.enabled=false"));
            }
        }
        if profile.lean_worker {
            for feat in crate::agents::lean_codex_disabled_features_if_enabled() {
                inner_args.push("--disable".to_string());
                inner_args.push(feat);
            }
        }
        if let Some(instr) = profile
            .developer_instructions
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            inner_args.push("-c".to_string());
            inner_args.push(format!(
                "developer_instructions={}",
                crate::agents::codex_toml_string(instr)
            ));
        }
    } else if spec.family == "antigravity" {
        if let Some(m) = model {
            inner_args.push("--model".to_string());
            inner_args.push(m.to_string());
        }
        if let Some(e) = effort {
            inner_args.push("--effort".to_string());
            inner_args.push(e.to_string());
        }
    }

    let codex_deny = if spec.family == "codex" {
        crate::agents::codex_deny_paths_for_session(cwd, profile.gbot_chat, profile.gbot_api_chat)
    } else {
        None
    };

    if read_only {
        if spec.family == "claude" {
            inner_args.push("--permission-mode".to_string());
            inner_args.push("plan".to_string());
        } else if spec.family == "codex" {
            inner_args.push("--sandbox".to_string());
            inner_args.push("read-only".to_string());
            inner_args.push("--ask-for-approval".to_string());
            inner_args.push("on-request".to_string());
        }
    } else if let Some(flag) = spec.cli_accept_all_flag {
        if accept_all && !(spec.family == "codex" && codex_deny.is_some()) {
            inner_args.push(flag.to_string());
        }
    }

    if let Some((is_api, deny_paths)) = codex_deny {
        let denies: Vec<String> = deny_paths
            .iter()
            .map(|p| format!("{}=\"deny\"", crate::agents::codex_toml_string(p)))
            .collect();
        let (prof_name, prof_val) = if is_api {
            let mut fs_entries = vec!["\"/\"=\"read\"".to_string()];
            if !read_only {
                if let Some(c) = cwd.map(str::trim).filter(|s| !s.is_empty()) {
                    fs_entries.push(format!("{}=\"write\"", crate::agents::codex_toml_string(c)));
                }
            }
            fs_entries.extend(denies);
            (
                "ghostex-gbot-api",
                format!(
                    "{{filesystem={{{}}}, network={{enabled={}}}}}",
                    fs_entries.join(", "),
                    if read_only { "false" } else { "true" }
                ),
            )
        } else if read_only {
            let mut fs_entries = vec!["\"/\"=\"read\"".to_string()];
            fs_entries.extend(denies);
            (
                "ghostex-home",
                format!(
                    "{{filesystem={{{}}}, network={{enabled=false}}}}",
                    fs_entries.join(", ")
                ),
            )
        } else if accept_all {
            inner_args.push("--ask-for-approval".to_string());
            inner_args.push("never".to_string());
            let mut fs_entries = vec!["\"/\"=\"write\"".to_string()];
            fs_entries.extend(denies);
            (
                "ghostex-home",
                format!(
                    "{{filesystem={{{}}}, network={{enabled=true}}}}",
                    fs_entries.join(", ")
                ),
            )
        } else {
            (
                "ghostex-home",
                format!(
                    "{{extends=\":workspace\", filesystem={{{}}}, network={{enabled=false}}}}",
                    denies.join(", ")
                ),
            )
        };
        inner_args.push("-c".to_string());
        inner_args.push(format!("permissions.{prof_name}={prof_val}"));
        inner_args.push("-c".to_string());
        inner_args.push(format!("default_permissions=\"{prof_name}\""));
    }

    if let Some(wrapper) = wrapper {
        let mut args = vec!["run".to_string(), wrapper.slot];
        if spec.family == "claude" {
            args.push("--share-history".to_string());
        }
        if !inner_args.is_empty() {
            args.push("--".to_string());
            args.extend(inner_args);
        }
        return (envs, wrapper.bin, args);
    }

    (envs, spec.default_cli_bin.to_string(), inner_args)
}
