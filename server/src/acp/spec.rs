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
    // `--cli_dir_name ghostex-acp` keeps `agy acp` on its default `request-review` policy
    // (isolated from any `toolPermission: always-proceed` in the user's CLI settings.json) while
    // sharing the user's existing login and `~/.gemini/antigravity-cli/brain/` transcripts so
    // Ghostex's four live approval levels (Ask, Auto-edits, Auto, Full access) are authoritative.
    acp_argv: &["agy", "acp", "--stdio", "--cli_dir_name", "ghostex-acp"],
    default_cli_bin: "agy",
    cli_resume_flag: "--conversation",
    cli_accept_all_flag: Some("--dangerously-skip-permissions"),
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

/// Build the command that `zmx`/`wmx` runs inside the session PTY to start `ghostex acp-host`.
pub fn build_acp_host_launch_command(
    family: &str,
    cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    approval_level: super::AcpApprovalLevel,
    writable_roots: &[String],
    model: Option<&str>,
    effort: Option<&str>,
) -> String {
    build_acp_host_launch_command_with_options(
        family,
        cli_command,
        resume_session_id,
        approval_level,
        writable_roots,
        model,
        effort,
        false,
        false,
    )
}

/// Build the `ghostex acp-host` launch command with optional `--fork-session` (for side chats and
/// session forks) and `--read-only` (for `start-thread --read-only`).
#[allow(clippy::too_many_arguments)]
pub fn build_acp_host_launch_command_with_options(
    family: &str,
    cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    approval_level: super::AcpApprovalLevel,
    writable_roots: &[String],
    model: Option<&str>,
    effort: Option<&str>,
    fork_session: bool,
    read_only: bool,
) -> String {
    build_acp_host_launch_command_for_platform(
        family,
        cli_command,
        resume_session_id,
        approval_level,
        writable_roots,
        model,
        effort,
        fork_session,
        read_only,
        cfg!(windows),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_acp_host_launch_command_for_platform(
    family: &str,
    cli_command: Option<&str>,
    resume_session_id: Option<&str>,
    approval_level: super::AcpApprovalLevel,
    writable_roots: &[String],
    model: Option<&str>,
    effort: Option<&str>,
    fork_session: bool,
    read_only: bool,
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

fn is_account_wrapper_bin(token: &str) -> bool {
    let base = token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "cswap" | "cswap.exe" | "xswap" | "xswap.exe"
    )
}

/// Sanitize a Claude inline `--settings` JSON object: strip `permissions.allow`,
/// `permissions.defaultMode`, and `permissions.additionalDirectories` so an agent-supplied or
/// inherited command can never loosen Ghostex's approval level, while preserving deny rules
/// (home-secret-folder denies, GBot computer-use denies) and non-permission settings (hooks,
/// compaction). Returns `None` for non-inline file paths so arbitrary settings files cannot be
/// smuggled into Non-CLI mode.
fn sanitize_claude_inline_settings(raw: &str) -> Option<String> {
    let Ok(serde_json::Value::Object(mut obj)) = serde_json::from_str::<serde_json::Value>(raw.trim())
    else {
        return None;
    };
    if let Some(perms) = obj
        .get_mut("permissions")
        .and_then(serde_json::Value::as_object_mut)
    {
        perms.remove("allow");
        perms.remove("defaultMode");
        perms.remove("additionalDirectories");
    }
    serde_json::to_string(&serde_json::Value::Object(obj)).ok()
}

/// Extract safe pass-through flags (`--settings <sanitized-inline-json>`, `--add-dir <val>`,
/// `--append-system-prompt`, `--append-system-prompt-file`) from a Claude CLI command so
/// home-secret-folder denial, GBot computer-use denial, Ghostex skills, and compaction settings
/// stay active in Non-CLI mode without leaking conflicting permission/bypass flags.
fn preserved_claude_flags(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        if tok == "--settings" {
            if let Some(val) = tokens.get(i + 1) {
                if let Some(sanitized) = sanitize_claude_inline_settings(val) {
                    out.push("--settings".to_string());
                    out.push(sanitized);
                }
                i += 2;
                continue;
            }
        } else if let Some(val) = tok.strip_prefix("--settings=") {
            if let Some(sanitized) = sanitize_claude_inline_settings(val) {
                out.push("--settings".to_string());
                out.push(sanitized);
            }
            i += 1;
            continue;
        } else if matches!(
            tok,
            "--add-dir" | "--append-system-prompt" | "--append-system-prompt-file"
        ) {
            if let Some(val) = tokens.get(i + 1) {
                out.push(tok.to_string());
                out.push(val.clone());
                i += 2;
                continue;
            }
        } else if tok.starts_with("--add-dir=")
            || tok.starts_with("--append-system-prompt=")
            || tok.starts_with("--append-system-prompt-file=")
        {
            out.push(tok.to_string());
        }
        i += 1;
    }
    out
}

/// True when a Codex `-c <expr>` override is one of Ghostex's security/sandbox/feature controls
/// (home-secret-folder denies, GBot API chat sandbox, GBot computer-use MCP server disables) and
/// NOT an untrusted `approval_policy` or `sandbox_mode` override.
fn is_safe_ghostex_codex_config_override(val: &str) -> bool {
    let trimmed = val.trim();
    if trimmed.starts_with("permissions.ghostex-") {
        return true;
    }
    if trimmed.starts_with("default_permissions=\"ghostex-")
        || trimmed.starts_with("default_permissions=ghostex-")
    {
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("mcp_servers.") {
        if let Some(name) = rest.strip_suffix(".enabled=false") {
            return !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        }
    }
    false
}

/// Extract Ghostex's safe Codex CLI flags (`-c permissions.ghostex-*`,
/// `-c default_permissions="ghostex-*"`, `-c mcp_servers.*.enabled=false`, and
/// `--disable <feature>`) so Non-CLI Codex keeps every Ghostex sandbox, secret-folder denial,
/// GBot computer-use restriction, and lean-worker flag without forwarding untrusted approval or
/// sandbox overrides.
fn preserved_codex_flags(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        if matches!(tok, "-c" | "--config") {
            if let Some(val) = tokens.get(i + 1) {
                if is_safe_ghostex_codex_config_override(val) {
                    out.push("-c".to_string());
                    out.push(val.clone());
                }
                i += 2;
                continue;
            }
        } else if let Some(val) = tok
            .strip_prefix("-c=")
            .or_else(|| tok.strip_prefix("--config="))
        {
            if is_safe_ghostex_codex_config_override(val) {
                out.push("-c".to_string());
                out.push(val.to_string());
            }
            i += 1;
            continue;
        } else if tok == "--disable" {
            if let Some(feature) = tokens.get(i + 1) {
                if !feature.is_empty() && !feature.starts_with('-') {
                    out.push("--disable".to_string());
                    out.push(feature.clone());
                }
                i += 2;
                continue;
            }
        } else if let Some(feature) = tok.strip_prefix("--disable=") {
            if !feature.is_empty() && !feature.starts_with('-') {
                out.push("--disable".to_string());
                out.push(feature.to_string());
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

/// Build `(envs, program, args)` to launch the agent's stdio ACP/SDK subprocess, respecting custom
/// executables and account wrappers (`cswap run <N> --share-history -- ...`, `xswap run <N> -- ...`).
pub fn build_acp_server_argv(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
) -> Option<(Vec<(String, String)>, String, Vec<String>)> {
    let (default_prog, acp_args) = spec.acp_argv.split_first()?;
    let (envs, base_tokens) = cli_command
        .map(tokenize_cli_command)
        .map(split_leading_env_assignments)
        .unwrap_or_default();

    // Preserve `cswap` / `xswap` account wrappers cleanly (and ALWAYS keep `--share-history` on
    // `cswap` so shared Claude projects/history are never unlinked).
    if let Some(first) = base_tokens.first().filter(|t| is_account_wrapper_bin(t)) {
        let is_cswap = first
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(first)
            .to_ascii_lowercase()
            .starts_with("cswap");
        let sep_idx = base_tokens.iter().position(|t| t == "--");
        let mut args = Vec::new();
        let tail_slice: &[String];
        if let Some(sep) = sep_idx {
            let mut prefix = base_tokens[1..sep].to_vec();
            if is_cswap && !prefix.iter().any(|t| t == "--share-history") {
                prefix.push("--share-history".to_string());
            }
            args.extend(prefix);
            args.push("--".to_string());
            tail_slice = &base_tokens[sep + 1..];
        } else {
            let mut prefix = base_tokens[1..].to_vec();
            if is_cswap && !prefix.iter().any(|t| t == "--share-history") {
                prefix.push("--share-history".to_string());
            }
            args.extend(prefix);
            args.push("--".to_string());
            tail_slice = &[];
        }
        // Both `cswap run <N> --share-history -- <args>` and `xswap run <N> -- <args>` pass
        // everything after `--` directly to `claude` or `codex` (never the binary name itself).
        args.extend(acp_args.iter().map(|s| (*s).to_string()));
        if spec.family == "claude" {
            args.extend(preserved_claude_flags(tail_slice));
        } else if spec.family == "codex" {
            args.extend(preserved_codex_flags(tail_slice));
        }
        return Some((envs, first.clone(), args));
    }

    let program = base_tokens
        .first()
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| (*default_prog).to_string());
    let mut args: Vec<String> = acp_args.iter().map(|s| (*s).to_string()).collect();
    if spec.family == "claude" && base_tokens.len() > 1 {
        args.extend(preserved_claude_flags(&base_tokens[1..]));
    } else if spec.family == "codex" && base_tokens.len() > 1 {
        args.extend(preserved_codex_flags(&base_tokens[1..]));
    }
    Some((envs, program, args))
}

/// Strip permissive approval/sandbox flags from CLI arguments when switching to Terminal while
/// downgraded below `Full access` (e.g. `Ask`), keeping Ghostex deny profiles and other flags.
fn strip_permissive_cli_flags(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let tok = args[i].as_str();
        let lower = tok.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "--dangerously-skip-permissions"
                | "--dangerously-bypass-approvals-and-sandbox"
                | "--yolo"
                | "--full-auto"
                | "--allow-all"
                | "--bypass-permissions"
                | "--accept-all"
        ) {
            i += 1;
            continue;
        }
        if matches!(
            lower.as_str(),
            "--permission-mode" | "--approval-mode" | "--ask-for-approval" | "-a" | "--sandbox" | "-s"
        ) {
            i += 2;
            continue;
        }
        if lower.starts_with("--permission-mode=")
            || lower.starts_with("--approval-mode=")
            || lower.starts_with("--ask-for-approval=")
            || lower.starts_with("-a=")
            || lower.starts_with("--sandbox=")
            || lower.starts_with("-s=")
        {
            i += 1;
            continue;
        }
        out.push(args[i].clone());
        i += 1;
    }
    out
}

/// Build `(envs, program, args)` to exec in-place inside `zmx`/`wmx` when the user switches an ACP
/// session to the interactive CLI terminal, preserving `--permission-mode plan` /
/// `--sandbox read-only` for `--read-only` threads and stripping non-bypass permissive flags when
/// `!accept_all`.
pub fn build_cli_switch_argv_with_options(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
    native_session_id: Option<&str>,
    accept_all: bool,
    read_only: bool,
) -> (Vec<(String, String)>, String, Vec<String>) {
    let (envs, mut base_tokens) = cli_command
        .map(tokenize_cli_command)
        .map(split_leading_env_assignments)
        .unwrap_or_default();
    if base_tokens.is_empty() {
        base_tokens.push(spec.default_cli_bin.to_string());
    }
    let program = base_tokens[0].clone();
    let is_wrapper = is_account_wrapper_bin(&program);
    let is_cswap = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&program)
        .to_ascii_lowercase()
        .starts_with("cswap");

    let (mut wrapper_prefix, mut inner_args) = if is_wrapper {
        if let Some(sep) = base_tokens.iter().position(|t| t == "--") {
            (base_tokens[1..sep].to_vec(), base_tokens[sep + 1..].to_vec())
        } else {
            (base_tokens[1..].to_vec(), Vec::new())
        }
    } else {
        (Vec::new(), base_tokens[1..].to_vec())
    };

    if is_cswap && !wrapper_prefix.iter().any(|t| t == "--share-history") {
        wrapper_prefix.push("--share-history".to_string());
    }

    if !accept_all || read_only {
        inner_args = strip_permissive_cli_flags(inner_args);
    }

    if let Some(id) = native_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        if spec.family == "codex" {
            // `codex resume [OPTIONS] [SESSION_ID]` expects `resume` as the subcommand before
            // resume-specific flags.
            if !inner_args.iter().any(|arg| arg == spec.cli_resume_flag) {
                let mut next = vec![spec.cli_resume_flag.to_string(), id.to_string()];
                next.extend(inner_args);
                inner_args = next;
            }
        } else if !inner_args.iter().any(|arg| arg == spec.cli_resume_flag) {
            inner_args.push(spec.cli_resume_flag.to_string());
            inner_args.push(id.to_string());
        }
    }

    if read_only {
        if spec.family == "claude" {
            inner_args.push("--permission-mode".to_string());
            inner_args.push("plan".to_string());
        } else if spec.family == "codex" {
            inner_args.push("--sandbox".to_string());
            inner_args.push("read-only".to_string());
        }
    } else if let Some(flag) = spec.cli_accept_all_flag {
        if accept_all {
            if !inner_args.iter().any(|arg| arg == flag) {
                inner_args.push(flag.to_string());
            }
        } else {
            inner_args.retain(|arg| arg != flag);
        }
    }

    let mut args = Vec::new();
    if is_wrapper {
        args.extend(wrapper_prefix);
        if !inner_args.is_empty() {
            args.push("--".to_string());
            args.extend(inner_args);
        }
    } else {
        args.extend(inner_args);
    }
    (envs, program, args)
}
