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
    // `--permission-mode` is added by `driver_claude.rs` only when Ghostex enforces an explicit
    // approval level or `--read-only`; omitting it under Keep default lets Claude resolve its own
    // `permissions.defaultMode` across user, project, local, and managed settings.
    acp_argv: &[
        "claude",
        "--output-format",
        "stream-json",
        "--verbose",
        "--input-format",
        "stream-json",
        "--permission-prompt-tool=stdio",
        "--include-partial-messages",
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

const OPENCODE_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "opencode",
    display_name: "OpenCode",
    // Native OpenCode 2 local HTTP+SSE server (`opencode serve`).
    acp_argv: &["opencode", "serve"],
    default_cli_bin: "opencode",
    cli_resume_flag: "--session",
    cli_accept_all_flag: Some("--auto"),
    supports_mid_turn_prompt: true,
};

const GROK_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "grok",
    display_name: "Grok Build",
    // Native Grok CLI ACP server over stdio with xAI extensions (`driver_grok`).
    acp_argv: &["grok", "--permission-mode", "default", "agent", "stdio"],
    default_cli_bin: "grok",
    cli_resume_flag: "-r",
    cli_accept_all_flag: Some("--always-approve"),
    supports_mid_turn_prompt: true,
};

const HERMES_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "hermes",
    display_name: "Hermes Agent",
    // Native Hermes ACP server over stdio, sharing the Hermes profile gateway and state.db.
    acp_argv: &["hermes", "acp"],
    default_cli_bin: "hermes",
    cli_resume_flag: "--resume",
    cli_accept_all_flag: Some("--yolo"),
    supports_mid_turn_prompt: true,
};

const CURSOR_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "cursor",
    display_name: "Cursor CLI",
    // Native Cursor Agent CLI ACP JSON-RPC over stdio (`cursor-agent acp`), driven by
    // `driver_cursor/` for Cursor's custom RPCs (`cursor/ask_question`, `cursor/create_plan`),
    // parameterized model picker, isolated approval config, and store/transcript bridging.
    acp_argv: &["cursor-agent", "acp"],
    default_cli_bin: "cursor-agent",
    cli_resume_flag: "--resume",
    cli_accept_all_flag: Some("--yolo"),
    // `cursor-agent acp` cancels an in-flight prompt if a second `session/prompt` arrives, so
    // Ghostex queues mid-turn prompts and delivers them after the active turn finishes.
    supports_mid_turn_prompt: false,
};

const PI_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "pi",
    display_name: "Pi Agent",
    // Native Pi RPC JSONL protocol over stdio (`driver_pi`).
    acp_argv: &["pi", "--mode", "rpc"],
    default_cli_bin: "pi",
    cli_resume_flag: "--session",
    cli_accept_all_flag: None,
    supports_mid_turn_prompt: true,
};

const OMP_SPEC: AcpAgentSpec = AcpAgentSpec {
    family: "omp",
    display_name: "OMP",
    // Native OMP ACP server over stdio (`omp acp`).
    acp_argv: &["omp", "acp"],
    default_cli_bin: "omp",
    cli_resume_flag: "--session",
    cli_accept_all_flag: None,
    supports_mid_turn_prompt: true,
};

pub fn spec_for_family(family: &str) -> Option<&'static AcpAgentSpec> {
    match family.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "antigravity-cli" | "agy" => Some(&ANTIGRAVITY_SPEC),
        "claude" | "claude-code" => Some(&CLAUDE_SPEC),
        "codex" | "codex-cli" => Some(&CODEX_SPEC),
        "opencode" | "opencode-cli" => Some(&OPENCODE_SPEC),
        "grok" | "grok-build" => Some(&GROK_SPEC),
        "hermes" | "hermes-agent" => Some(&HERMES_SPEC),
        "cursor" | "cursor-cli" | "cursor-agent" => Some(&CURSOR_SPEC),
        "pi" | "pi-agent" => Some(&PI_SPEC),
        "omp" | "oh-my-pi" => Some(&OMP_SPEC),
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
        "opencode" => crate::session_chat_opencode::safe_id(native_session_id)
            .then(|| super::driver_opencode::mirror_path(native_session_id)),
        "grok" => crate::session_chat_paths::resolve_session_chat_transcript_path(
            crate::session_chat::SessionChatTranscriptAgent::Grok,
            Some(native_session_id),
            None,
        ),
        "hermes" => super::hermes::expected_transcript_mirror_path(native_session_id),
        "cursor" => super::driver_cursor::cursor_native_transcript_path(native_session_id)
            .filter(|p| p.is_file())
            .or_else(|| {
                crate::session_chat_paths::resolve_session_chat_transcript_path(
                    crate::session_chat::SessionChatTranscriptAgent::Cursor,
                    Some(native_session_id),
                    None,
                )
            })
            .or_else(|| super::driver_cursor::cursor_native_transcript_path(native_session_id)),
        "pi" | "omp" => crate::session_chat_paths::resolve_session_chat_transcript_path(
            crate::session_chat::SessionChatTranscriptAgent::Pi,
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
    pub claude_chrome: bool,
    /// The chat turned Chrome off (`--no-chrome`), whatever its account's default (claude_chrome.rs).
    pub claude_chrome_off: bool,
    pub hermes_profile: Option<String>,
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
    approval_level: Option<super::AcpApprovalLevel>,
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
    approval_level: Option<super::AcpApprovalLevel>,
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
    if let Some(level) = approval_level {
        parts.push("--approval-level".to_string());
        parts.push(quote_acp_launch_arg(level.as_str(), windows));
    }
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
    if family == "claude" && profile.claude_chrome {
        parts.push("--chrome".to_string());
    } else if family == "claude" && profile.claude_chrome_off {
        parts.push("--no-chrome".to_string());
    }
    if let Some(hermes_profile) = profile
        .hermes_profile
        .as_deref()
        .map(str::trim)
        .filter(|p| super::hermes::is_valid_hermes_profile(p))
    {
        parts.push("--hermes-profile".to_string());
        parts.push(quote_acp_launch_arg(hermes_profile, windows));
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
    if !matches!(
        family,
        "claude"
            | "codex"
            | "antigravity"
            | "opencode"
            | "grok"
            | "hermes"
            | "cursor"
            | "pi"
            | "omp"
    ) {
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
        } else if let Some(v) = tok
            .strip_prefix("--effort=")
            .or_else(|| {
                (family == "grok")
                    .then(|| tok.strip_prefix("--reasoning-effort="))
                    .flatten()
            })
            .or_else(|| {
                (family == "hermes")
                    .then(|| tok.strip_prefix("--reasoning="))
                    .flatten()
            })
            .or_else(|| {
                matches!(family, "pi" | "omp")
                    .then(|| tok.strip_prefix("--thinking="))
                    .flatten()
            })
        {
            if effort.is_none() && is_safe_model_or_effort_token(v) {
                effort = Some(v.trim().to_string());
            }
        } else if tok == "--effort"
            || (family == "grok" && tok == "--reasoning-effort")
            || (family == "hermes" && tok == "--reasoning")
            || (matches!(family, "pi" | "omp") && tok == "--thinking")
        {
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
        "opencode" => "opencode",
        "grok" => "grok",
        "hermes" => "hermes",
        "cursor" => "cursor-agent",
        "pi" => "pi",
        "omp" => "omp",
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
            .or_else(|| {
                (family == "grok")
                    .then(|| tok.strip_prefix("--reasoning-effort="))
                    .flatten()
            })
            .or_else(|| {
                (family == "hermes")
                    .then(|| tok.strip_prefix("--reasoning="))
                    .flatten()
            })
            .or_else(|| {
                matches!(family, "pi" | "omp")
                    .then(|| tok.strip_prefix("--thinking="))
                    .flatten()
            })
        {
            if !is_safe_model_or_effort_token(v) {
                ignored.push(tok.to_string());
            }
        } else if matches!(tok, "--model" | "-m" | "--effort")
            || (family == "grok" && tok == "--reasoning-effort")
            || (family == "hermes" && tok == "--reasoning")
            || (matches!(family, "pi" | "omp") && tok == "--thinking")
        {
            if let Some(next) = base_tokens.get(i + 1) {
                if !is_safe_model_or_effort_token(next) {
                    ignored.push(tok.to_string());
                    ignored.push(next.clone());
                }
                i += 1;
            } else {
                ignored.push(tok.to_string());
            }
        } else if family == "hermes" && tok.starts_with("--profile=") {
            let v = tok.trim_start_matches("--profile=");
            if !super::hermes::is_valid_hermes_profile(v) {
                ignored.push(tok.to_string());
            }
        } else if family == "hermes" && matches!(tok, "-p" | "--profile") {
            if let Some(next) = base_tokens.get(i + 1) {
                if !super::hermes::is_valid_hermes_profile(next) {
                    ignored.push(tok.to_string());
                    ignored.push(next.clone());
                }
                i += 1;
            } else {
                ignored.push(tok.to_string());
            }
        } else if let width @ 1.. =
            full_access_flag_width(family, tok, base_tokens.get(i + 1).map(String::as_str))
        {
            // Honored as Full permissions (`owner_command_requests_full_access`), not ignored.
            i += width - 1;
        } else {
            ignored.push(tok.to_string());
        }
        i += 1;
    }
    ignored
}

/// How many tokens at `tok` (1, or 2 for `--permission-mode bypassPermissions`) start the
/// family's CLI with approvals off; 0 when `tok` is not such a flag.
pub(crate) fn full_access_flag_width(family: &str, tok: &str, next: Option<&str>) -> usize {
    match family {
        "claude" => match tok {
            "--dangerously-skip-permissions" | "--permission-mode=bypassPermissions" => 1,
            "--permission-mode" if next == Some("bypassPermissions") => 2,
            _ => 0,
        },
        "codex" => match tok {
            "--dangerously-bypass-approvals-and-sandbox" | "--yolo" => 1,
            _ => 0,
        },
        "opencode" => match tok {
            "--auto" | "--yolo" | "--dangerously-skip-permissions" => 1,
            _ => 0,
        },
        "grok" => match tok {
            "--always-approve" | "--permission-mode=bypassPermissions" => 1,
            "--permission-mode" if next == Some("bypassPermissions") => 2,
            _ => 0,
        },
        "hermes" => match tok {
            "--yolo" => 1,
            _ => 0,
        },
        "cursor" => match tok {
            "--yolo" | "--force" | "-f" => 1,
            _ => 0,
        },
        _ => 0,
    }
}

/// True when the owner's saved agent command (Settings > Agents, `agent_config.command`) starts
/// the CLI with approvals off: Claude `--dangerously-skip-permissions` (or `--permission-mode
/// bypassPermissions`), Codex `--dangerously-bypass-approvals-and-sandbox` (or `--yolo`).
///
/// CDXC:NativeApprovals 2026-11-25 DECISION:
/// Brief (12.1.1 patch, Discord report "with 12.0 it keeps asking permission for every single tool call since i turned it to native mode"): a bypass flag typed into the agent's command in Settings > Agents was dropped by Native with only an "ignored" note, so Native asked before every tool while the same command ran without asking in Terminal mode. Native now reads that flag as the agent's Full permissions choice at launch and resume, in the same place and order Terminal mode applies it (`resolve_session_launch_command_for_workspace`): a per-chat or per-agent Permission mode of Keep default or Auto still wins (Terminal strips the flag for those), as does a level picked in the chat's Mode menu. Chosen over a one-time migration into Permission mode: Full permissions because it rewrites nothing the user typed, keeps the command identical in both modes, and needs no settings sync or phone change; only the owner's own devices can save such a command (`sidebar_hud/mutations.rs` refuses it from agents and peers).
pub(crate) fn owner_command_requests_full_access(
    family: &str,
    owner_command: Option<&str>,
) -> bool {
    let Some(raw) = owner_command.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let tokens = tokenize_cli_command(raw);
    let (_, base_tokens) = split_leading_env_assignments(tokens);
    (1..base_tokens.len()).any(|i| {
        full_access_flag_width(
            family,
            base_tokens[i].as_str(),
            base_tokens.get(i + 1).map(String::as_str),
        ) > 0
    })
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
    // The home `apply` wrote the folder under (accounts/hub/launch.rs): a separate profile's own
    // home, else the user's. With no home at all there is no folder to name, as before.
    let home = crate::paths::get_gxserver_paths(None)
        .agent_config_home_dir()
        .to_path_buf();
    if !home.is_absolute() {
        return None;
    }
    Some(crate::accounts::hub::launch::profile_dir(
        &home,
        provider,
        hub_id.trim(),
    ))
}

/// CDXC:AgentProviders 2026-11-26 WHY:
/// GitHub #773: Claude Code labels a run started with `--output-format stream-json` (or `-p`) `sdk-cli` unless `CLAUDE_CODE_ENTRYPOINT` already names one, so every Native Claude chat went out as `sdk-cli`, in its transcript (`"entrypoint":"sdk-cli"`) and in the `cc_entrypoint` of its requests' billing header, while a Terminal-based chat is `cli`. An open Claude Code issue (anthropics/claude-code#97074) reports that `sdk-cli` and Agent SDK usage counts about 1.8x against the 5-hour limit. A host that names itself keeps its own name: Claude 2.1.332 only rewrites an inherited `cli` to `sdk-cli`, and its own SDK sets `sdk-ts` only when the variable is unset. So Native Claude runs under Ghostex's own name, over whatever the host process inherited. Anthropic does not document how each name weighs against the limit; the label is what this changes.
/// SEE-ALSO: server/src/agent_hooks/nested_agent.rs `is_headless_claude_entrypoint` (a Native Claude's hooks, and a `claude -p` its tools start, inherit this name and stay headless).
pub(crate) const GHOSTEX_CLAUDE_ENTRYPOINT: &str = "ghostex";

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
    if spec.family == "claude" {
        envs.push((
            "CLAUDE_CODE_ENTRYPOINT".to_string(),
            GHOSTEX_CLAUDE_ENTRYPOINT.to_string(),
        ));
    }
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

    let program =
        windows_batch_program(default_prog).unwrap_or_else(|| (*default_prog).to_string());
    let mut args: Vec<String> = Vec::with_capacity(acp_args.len() + 2);
    if spec.family == "hermes" {
        if let Some(hermes_profile) = profile
            .hermes_profile
            .as_deref()
            .map(str::trim)
            .filter(|p| super::hermes::is_valid_hermes_profile(p))
        {
            args.push("-p".to_string());
            args.push(hermes_profile.to_string());
        }
    }
    args.extend(acp_args.iter().map(|s| (*s).to_string()));
    Some((envs, program, args))
}

/// The full path of `name`'s `.cmd` or `.bat` launcher on Windows, when that is what the PATH has.
///
/// CDXC:PlatformSupport 2026-11-24 WHY:
/// GitHub #737: Rust's `Command::new("claude")` on Windows looks only for `claude.exe`, so on a PC where Claude Code came from npm (only `claude.cmd` on PATH) every Native session failed with "program not found". A batch file found this way is run by its path; Rust escapes its arguments for `cmd`, which carried Ghostex's `--settings` JSON through npm's `%*` intact (checked with Claude Code 2.1.331 on 2026-11-24).
fn windows_batch_program(name: &str) -> Option<String> {
    if !cfg!(windows) || cfg!(test) {
        return None;
    }
    // The PATH `Command` searches: this process's own, not the registry's (`live_path::find`).
    let directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let path = crate::platform::live_path::find_in(name, &directories)?;
    let extension = path.extension().and_then(|extension| extension.to_str())?;
    (extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat"))
        .then(|| path.to_string_lossy().into_owned())
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

/// The interactive CLI's flags for a Native session's Auto or Auto-edits level, on a switch to
/// Terminal-based. Ask (and an agent without these modes) keeps the CLI's own default; Full access
/// is the agent's accept-all flag.
///
/// CDXC:AgentProviders 2026-11-25 WHY:
/// The mirror of GitHub #746 (`terminal_approval_level` in channel.rs, Terminal -> Native): a Native chat in Auto or Auto-edits switched to Terminal-based restarted its CLI in the agent's default mode and asked before each edit or command. The flags are the ones Terminal-based launches already use for these levels: Auto is Settings > Agents Permission mode "Auto" (`apply_auto_permission_mode` in agents/accept_all.rs: Claude `--permission-mode auto`, Codex `--approve-for-me`), Auto-edits is a GBot chat's "edits" launch (`apply_approval` in gbots/session.rs: Claude `--permission-mode acceptEdits`, Codex `--sandbox workspace-write --ask-for-approval on-request`).
fn cli_switch_permission_args(
    family: &str,
    level: super::AcpApprovalLevel,
) -> &'static [&'static str] {
    use super::AcpApprovalLevel::{Auto, AutoEdits};
    match (family, level) {
        ("claude", Auto) => &["--permission-mode", "auto"],
        ("claude", AutoEdits) => &["--permission-mode", "acceptEdits"],
        ("grok", Auto) => &["--permission-mode", "auto"],
        ("grok", AutoEdits) => &["--permission-mode", "acceptEdits"],
        ("codex", Auto) => &["--approve-for-me"],
        ("codex", AutoEdits) => &[
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "on-request",
        ],
        _ => &[],
    }
}

/// Build `(envs, program, args)` to exec in-place inside `zmx`/`wmx` when the user switches an ACP
/// session to the interactive CLI terminal. Builds the interactive CLI strictly from Ghostex's
/// structured `AcpLaunchProfile`, preserving `--permission-mode plan` /
/// `--sandbox read-only --ask-for-approval on-request` for `--read-only` threads and re-applying
/// Ghostex's own deny profiles cleanly for the session's current approval level (never leaking
/// untrusted command flags, arbitrary binary paths, or a full-write `ghostex-home` profile on
/// downgrade). The session's approval level picks the CLI's permission flags
/// ([`cli_switch_permission_args`]).
pub fn build_cli_switch_argv_with_options(
    spec: &AcpAgentSpec,
    _cli_command: Option<&str>,
    native_session_id: Option<&str>,
    approval_level: super::AcpApprovalLevel,
    read_only: bool,
    profile: &AcpLaunchProfile,
    cwd: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> (Vec<(String, String)>, String, Vec<String>) {
    let accept_all = approval_level.implies_cli_accept_all();
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
    if spec.family == "hermes" {
        if let Some(hermes_profile) = profile
            .hermes_profile
            .as_deref()
            .map(str::trim)
            .filter(|p| super::hermes::is_valid_hermes_profile(p))
        {
            inner_args.push("-p".to_string());
            inner_args.push(hermes_profile.to_string());
        }
    }
    if let Some(id) = native_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        if spec.family == "pi" {
            let existing_path = if std::path::Path::new(id).is_file() {
                Some(id.to_string())
            } else {
                transcript_path_for_session("pi", id)
                    .filter(|p| p.is_file())
                    .map(|p| p.to_string_lossy().to_string())
            };
            if let Some(path) = existing_path {
                inner_args.push("--session".to_string());
                inner_args.push(path);
            } else if crate::agents::pi_session_id::is_pi_session_id(id) {
                inner_args.push("--session-id".to_string());
                inner_args.push(id.to_string());
            } else {
                inner_args.push(spec.cli_resume_flag.to_string());
                inner_args.push(id.to_string());
            }
        } else {
            inner_args.push(spec.cli_resume_flag.to_string());
            inner_args.push(id.to_string());
        }
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
        if let Some(flag) = super::claude_chrome::cli_switch_flag(profile) {
            inner_args.push(flag.to_string());
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
    } else if spec.family == "grok" {
        if let Some(m) = model {
            inner_args.push("--model".to_string());
            inner_args.push(m.to_string());
        }
        if let Some(e) = effort {
            inner_args.push("--reasoning-effort".to_string());
            inner_args.push(e.to_string());
        }
    } else if matches!(spec.family, "pi" | "omp") {
        if let Some(m) = model {
            inner_args.push("--model".to_string());
            inner_args.push(m.to_string());
        }
        if let Some(e) = effort {
            inner_args.push("--thinking".to_string());
            inner_args.push(e.to_string());
        }
    } else if spec.family == "hermes" {
        // Hermes refuses `-m/--model` and `--reasoning` when `--resume` is passed because
        // the gateway session already retains its switched model and reasoning policy.
        let is_resume = native_session_id
            .map(str::trim)
            .is_some_and(|s| !s.is_empty());
        if !is_resume {
            if let Some(m) = model {
                inner_args.push("--model".to_string());
                inner_args.push(super::hermes::strip_hermes_provider_prefix(m).to_string());
            }
            if let Some(e) = effort.filter(|e| !e.eq_ignore_ascii_case("default")) {
                inner_args.push("--reasoning".to_string());
                inner_args.push(e.to_string());
            }
        }
    } else if spec.family == "cursor" {
        if let Some(m) = model {
            inner_args.push("--model".to_string());
            inner_args.push(m.to_string());
        }
    }

    let codex_deny = if spec.family == "codex" {
        crate::agents::codex_deny_paths_for_session(cwd, profile.gbot_chat, profile.gbot_api_chat)
    } else {
        None
    };

    if read_only {
        if spec.family == "claude" || spec.family == "grok" {
            inner_args.push("--permission-mode".to_string());
            inner_args.push("plan".to_string());
        } else if spec.family == "codex" {
            inner_args.push("--sandbox".to_string());
            inner_args.push("read-only".to_string());
            inner_args.push("--ask-for-approval".to_string());
            inner_args.push("on-request".to_string());
        } else if spec.family == "cursor" {
            inner_args.push("--mode".to_string());
            inner_args.push("plan".to_string());
        }
    } else if accept_all {
        if let Some(flag) = spec.cli_accept_all_flag {
            if !(spec.family == "codex" && codex_deny.is_some()) {
                inner_args.push(flag.to_string());
            }
        }
    } else {
        inner_args.extend(
            cli_switch_permission_args(spec.family, approval_level)
                .iter()
                .map(|arg| arg.to_string()),
        );
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
    // Last, as on a Terminal-based launch: Claude's `--mcp-config` takes several values.
    inner_args.extend(crate::connectors::native_connector_tool_args(spec.family));

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

    let cli_bin = if spec.family == "opencode" {
        let home = crate::resume_lookup::home_dir();
        crate::agent_cli::opencode::opencode2_program(&home)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| spec.default_cli_bin.to_string())
    } else {
        spec.default_cli_bin.to_string()
    };

    (envs, cli_bin, inner_args)
}
