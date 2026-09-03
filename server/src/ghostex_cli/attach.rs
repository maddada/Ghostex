use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::{launchers, picker, selector, sessions};

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI attach flow (scripts/ghostex-cli.mjs lines
4823-4933 and 5482-5570). `gx a <selector>` resolves the session, applies
gxserver attach metadata (start-missing-provider + restore-block checks live in
sessions::apply_attach_metadata), then execs the provider attach or the zmx
attach-or-resume bootstrap script through the user's interactive shell. Bare
`gx a` opens the lightweight picker, not the full TUI. The generated zmx
bootstrap script must stay byte-identical to the Node CLI because remote hosts,
Android, and the TUI all reattach through it.
*/

const CLI_POSIX_SHELL_NAMES: [&str; 7] = ["ash", "bash", "dash", "ksh", "mksh", "sh", "zsh"];
const CLI_LOGIN_COMMAND_SHELL_NAMES: [&str; 2] = ["bash", "zsh"];

pub fn attach_session_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    // Ghostex Android attaches by stable session id via --session-id; positional
    // selectors stay for human usage. Empty attach opens the fast picker.
    let selector_text = match parsed.flags.text("sessionId") {
        Some(value) => value,
        None => parsed.rest.join(" ").trim().to_string(),
    };
    if selector_text.is_empty() {
        return interactive_session_picker_command(args);
    }
    let session = selector::resolve_cli_session_selector(&selector_text, &parsed.flags)?;
    attach_resolved_session(session, &parsed.flags)
}

pub fn attach_resolved_session(mut session: Value, flags: &Flags) -> CliResult<()> {
    // Sleeping provider-backed rows prefer the agent resume command; awake rows
    // keep provider attach first. gxserver metadata decides which applies.
    sessions::apply_attach_metadata(&mut session, flags)?;
    let Some(command) = build_session_attach_command(&session) else {
        return Err(CliError::Other(format!(
            "Session {} has no provider attach command or supported agent resume command.",
            js_display(session.get("alias"))
        )));
    };
    let cwd: Option<PathBuf> = session
        .get("projectPath")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    launchers::run_interactive_shell_command(&command, cwd.as_deref())?;
    Ok(())
}

pub fn interactive_session_picker_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let sessions_list = sessions::fetch_session_list(&parsed.flags, true)?;
    if sessions_list.is_empty() {
        println!("No running terminal sessions.");
        return Ok(());
    }
    if !picker::is_interactive_terminal() {
        picker::print_session_picker_rows(&sessions_list);
        return Ok(());
    }
    let Some(session) = picker::interactive_session_picker(&sessions_list)? else {
        return Ok(());
    };
    attach_resolved_session(session, &parsed.flags)
}

pub fn build_session_attach_command(session: &Value) -> Option<String> {
    if should_create_missing_zmx_session_with_resume(session) {
        return Some(build_zmx_attach_or_resume_command(session));
    }
    let attach_command = session.get("attachCommand");
    let resume_command = session.get("resumeCommand");
    let is_sleeping = session.get("status").and_then(Value::as_str) == Some("sleep");
    if is_zmx_session(session) && !is_sleeping && js_truthy(attach_command) {
        // Live zmx attaches use the stored provider attach command for every
        // client so reattach restores full zmx state and scrollback.
        return Some(js_string(attach_command.expect("truthy value present")));
    }
    let ordered = if is_sleeping {
        [resume_command, attach_command]
    } else {
        [attach_command, resume_command]
    };
    ordered
        .iter()
        .find(|value| js_truthy(**value))
        .map(|value| js_string(value.expect("truthy value present")))
}

pub fn is_zmx_session(session: &Value) -> bool {
    js_string_or_empty(session.get("provider"))
        .trim()
        .to_lowercase()
        == "zmx"
}

pub fn should_create_missing_zmx_session_with_resume(session: &Value) -> bool {
    is_zmx_session(session)
        && !js_string_or_empty(session.get("providerSessionName"))
            .trim()
            .is_empty()
        && !js_string_or_empty(session.get("resumeCommand"))
            .trim()
            .is_empty()
}

pub fn build_zmx_attach_or_resume_command(session: &Value) -> String {
    // Mobile sidebar taps run through `ghostex attach --session-id`. If the
    // card's zmx session is gone but the agent has a resume command, recreate
    // the named zmx session and run the agent resume command there instead of
    // opening an attach terminal that immediately exits or becomes an empty shell.
    build_zmx_attach_or_resume_command_with(
        session,
        &resolve_cli_interactive_shell_launch(),
        cfg!(target_os = "macos"),
    )
}

fn build_zmx_attach_or_resume_command_with(
    session: &Value,
    shell: &CliShellLaunch,
    darwin: bool,
) -> String {
    let session_name = js_display(session.get("providerSessionName"))
        .trim()
        .to_string();
    let resume_command = js_display(session.get("resumeCommand")).trim().to_string();
    let resume_fallback_command = js_string_or_empty(session.get("resumeFallbackCommand"))
        .trim()
        .to_string();
    let cwd_value = session.get("projectPath");
    let cwd_raw = if js_truthy(cwd_value) {
        js_string(cwd_value.expect("truthy value present"))
    } else {
        ".".to_string()
    };
    let cwd = {
        let trimmed = cwd_raw.trim();
        if trimmed.is_empty() {
            "."
        } else {
            trimmed
        }
    };
    let keepalive_shell_assignment = if darwin {
        "zmx_keepalive_shell=${SHELL:-/bin/zsh}".to_string()
    } else {
        format!("zmx_keepalive_shell={}", js_shell_quote(&shell.executable))
    };
    let script = format!(
        "\nzmx_session={sq_session}\nzmx_resume_command={sq_resume}\nzmx_resume_fallback_command={sq_fallback}\nzmx_cwd={sq_cwd}\nzmx_resume_shell={sq_shell}\nzmx_resume_shell_flag={sq_shell_flag}\n{keepalive_shell_assignment}\nzmx_keepalive_shell_login_flag={sq_login_flag}\nexport zmx_resume_command zmx_resume_fallback_command zmx_resume_shell zmx_resume_shell_flag zmx_keepalive_shell zmx_keepalive_shell_login_flag\nunset ZMX_SESSION ZMX_SESSION_PREFIX\nif ! command -v zmx >/dev/null 2>&1; then\n  printf '%s\\n' 'zmx was not found on PATH.'\n  exit 127\nfi\nif zmx list --short 2>/dev/null | grep -F -x -- \"$zmx_session\" >/dev/null 2>&1; then\n  exec zmx attach \"$zmx_session\"\nfi\ncd \"$zmx_cwd\" || exit\nzmx_resume_launcher='\nset +e\n\"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_command\"\nzmx_resume_status=$?\nif [ \"$zmx_resume_status\" -ne 0 ] && [ -n \"$zmx_resume_fallback_command\" ] && [ \"$zmx_resume_fallback_command\" != \"$zmx_resume_command\" ]; then\n  printf '\"'\"'%s\\n'\"'\"' \"Exact resume failed; trying saved fallback resume command.\"\n  \"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_fallback_command\"\n  zmx_resume_status=$?\nfi\nif [ \"$zmx_resume_status\" -ne 0 ]; then\n  printf '\"'\"'\\n%s\\n'\"'\"' \"Resume command exited with status $zmx_resume_status. Leaving this pane open for inspection.\"\n  if [ -n \"$zmx_keepalive_shell_login_flag\" ]; then\n    exec \"$zmx_keepalive_shell\" \"$zmx_keepalive_shell_login_flag\"\n  fi\n  exec \"$zmx_keepalive_shell\"\nfi\nexit 0\n'\nexec zmx attach \"$zmx_session\" \"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_launcher\"\n",
        sq_session = js_shell_quote(&session_name),
        sq_resume = js_shell_quote(&resume_command),
        sq_fallback = js_shell_quote(&resume_fallback_command),
        sq_cwd = js_shell_quote(cwd),
        sq_shell = js_shell_quote(&shell.executable),
        sq_shell_flag = js_shell_quote(&shell.command_flag),
        sq_login_flag = js_shell_quote(&shell.login_flag),
    );
    cli_shell_command_string_with(&script, shell)
}

#[derive(Clone, Debug)]
struct CliShellLaunch {
    command_flag: String,
    executable: String,
    login_flag: String,
}

fn resolve_cli_interactive_shell_launch() -> CliShellLaunch {
    // Remote `ghostex attach` runs from the bundled CLI on the target machine,
    // so it must not spawn macOS-only /bin/zsh on Ubuntu. macOS stays pinned to
    // zsh for compatibility; Linux resolves an installed POSIX shell.
    if cfg!(target_os = "macos") {
        return CliShellLaunch {
            command_flag: "-lc".to_string(),
            executable: "/bin/zsh".to_string(),
            login_flag: "-l".to_string(),
        };
    }
    let candidates = cli_interactive_shell_candidates();
    let executable = candidates
        .iter()
        .find(|candidate| is_executable_file_sync(candidate))
        .cloned()
        .or_else(|| candidates.first().cloned())
        .unwrap_or_else(|| "/bin/sh".to_string());
    CliShellLaunch {
        command_flag: cli_shell_command_flag(&executable).to_string(),
        login_flag: cli_shell_login_flag(&executable).to_string(),
        executable,
    }
}

fn cli_interactive_shell_candidates() -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let shell = std::env::var("SHELL")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !shell.is_empty() && is_supported_cli_posix_shell(&shell) {
        candidates.push(shell);
    }
    for fallback in ["/bin/bash", "/usr/bin/bash", "/bin/sh", "/usr/bin/sh"] {
        candidates.push(fallback.to_string());
    }
    unique_strings(candidates)
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for value in values {
        let normalized = value.trim().to_string();
        if !normalized.is_empty() && !seen.contains(&normalized) {
            seen.push(normalized);
        }
    }
    seen
}

fn shell_basename_lowercase(shell_path: &str) -> String {
    Path::new(shell_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn is_supported_cli_posix_shell(shell_path: &str) -> bool {
    CLI_POSIX_SHELL_NAMES.contains(&shell_basename_lowercase(shell_path).as_str())
}

fn cli_shell_command_flag(shell_path: &str) -> &'static str {
    if CLI_LOGIN_COMMAND_SHELL_NAMES.contains(&shell_basename_lowercase(shell_path).as_str()) {
        "-lc"
    } else {
        "-c"
    }
}

fn cli_shell_login_flag(shell_path: &str) -> &'static str {
    if CLI_LOGIN_COMMAND_SHELL_NAMES.contains(&shell_basename_lowercase(shell_path).as_str()) {
        "-l"
    } else {
        ""
    }
}

#[allow(unused_variables)]
fn is_executable_file_sync(file_path: &str) -> bool {
    #[cfg(unix)]
    {
        let Ok(path) = std::ffi::CString::new(file_path.as_bytes()) else {
            return false;
        };
        unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        Path::new(file_path).exists()
    }
}

fn cli_shell_command_string_with(command: &str, shell: &CliShellLaunch) -> String {
    format!(
        "{} {} {}",
        shell_word(&shell.executable),
        shell.command_flag,
        js_shell_quote(command)
    )
}

/// Faithful port of the JS shellQuote: ALWAYS single-quotes (rpc::shell_quote
/// leaves plain words bare, which would change the generated script bytes).
fn js_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell_word(value: &str) -> String {
    let is_plain = !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-'
                )
        });
    if is_plain {
        value.to_string()
    } else {
        js_shell_quote(value)
    }
}

/// JS truthiness of an optional JSON value (missing/undefined → false).
fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// JS String(value) coercion for JSON values.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                int.to_string()
            } else if let Some(int) = number.as_u64() {
                int.to_string()
            } else {
                let float = number.as_f64().unwrap_or(0.0);
                if float.fract() == 0.0 && float.abs() < 9.007_199_254_740_992e15 {
                    format!("{}", float as i64)
                } else {
                    format!("{float}")
                }
            }
        }
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// String(value ?? "") — null/undefined become "".
fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(other) => js_string(other),
    }
}

/// Template interpolation of a possibly-missing value (undefined → "undefined").
fn js_display(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(other) => js_string(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn zsh_shell() -> CliShellLaunch {
        CliShellLaunch {
            command_flag: "-lc".to_string(),
            executable: "/bin/zsh".to_string(),
            login_flag: "-l".to_string(),
        }
    }

    #[test]
    fn attach_command_prefers_zmx_resume_bootstrap() {
        let session = json!({
            "provider": "zmx",
            "providerSessionName": "g-0713-1",
            "resumeCommand": "claude --resume abc",
            "attachCommand": "zmx attach g-0713-1",
            "projectPath": "/tmp/p",
        });
        let command = build_session_attach_command(&session).unwrap();
        assert!(
            command.starts_with("/bin/zsh -lc '")
                || command.contains(" -c '")
                || command.contains(" -lc '")
        );
        assert!(command.contains("zmx_session="));
    }

    #[test]
    fn attach_command_uses_live_attach_without_resume() {
        let session = json!({
            "provider": "zmx",
            "attachCommand": "zmx attach g-1",
            "status": "running",
        });
        assert_eq!(
            build_session_attach_command(&session),
            Some("zmx attach g-1".to_string())
        );
    }

    #[test]
    fn attach_command_sleep_prefers_resume_then_attach() {
        let session = json!({
            "status": "sleep",
            "resumeCommand": "claude --resume x",
            "attachCommand": "tmux attach -t a",
        });
        assert_eq!(
            build_session_attach_command(&session),
            Some("claude --resume x".to_string())
        );
        let session = json!({ "status": "sleep", "attachCommand": "tmux attach -t a" });
        assert_eq!(
            build_session_attach_command(&session),
            Some("tmux attach -t a".to_string())
        );
        let session = json!({ "status": "sleep" });
        assert_eq!(build_session_attach_command(&session), None);
    }

    #[test]
    fn attach_command_awake_prefers_attach_then_resume() {
        let session = json!({
            "attachCommand": "tmux attach -t a",
            "resumeCommand": "claude --resume x",
        });
        assert_eq!(
            build_session_attach_command(&session),
            Some("tmux attach -t a".to_string())
        );
        let session = json!({ "resumeCommand": "claude --resume x" });
        assert_eq!(
            build_session_attach_command(&session),
            Some("claude --resume x".to_string())
        );
    }

    #[test]
    fn zmx_session_detection_normalizes_provider() {
        assert!(is_zmx_session(&json!({ "provider": " ZMX " })));
        assert!(!is_zmx_session(&json!({ "provider": "tmux" })));
        assert!(!is_zmx_session(&json!({})));
        assert!(should_create_missing_zmx_session_with_resume(&json!({
            "provider": "zmx",
            "providerSessionName": "g-1",
            "resumeCommand": "claude",
        })));
        assert!(!should_create_missing_zmx_session_with_resume(&json!({
            "provider": "zmx",
            "providerSessionName": " ",
            "resumeCommand": "claude",
        })));
        assert!(!should_create_missing_zmx_session_with_resume(&json!({
            "provider": "zmx",
            "providerSessionName": "g-1",
        })));
    }

    // Expected strings generated by running the Node implementation
    // (buildZmxAttachOrResumeCommand + cliShellCommandString) on the same
    // fixture inputs; the Rust port must stay byte-identical.
    #[test]
    fn zmx_bootstrap_matches_node_output_darwin() {
        let session = json!({
            "providerSessionName": "g-0713-1",
            "resumeCommand": "claude --resume abc",
            "resumeFallbackCommand": "claude",
            "projectPath": "/tmp/my project",
        });
        let expected = "/bin/zsh -lc '\nzmx_session='\\''g-0713-1'\\''\nzmx_resume_command='\\''claude --resume abc'\\''\nzmx_resume_fallback_command='\\''claude'\\''\nzmx_cwd='\\''/tmp/my project'\\''\nzmx_resume_shell='\\''/bin/zsh'\\''\nzmx_resume_shell_flag='\\''-lc'\\''\nzmx_keepalive_shell=${SHELL:-/bin/zsh}\nzmx_keepalive_shell_login_flag='\\''-l'\\''\nexport zmx_resume_command zmx_resume_fallback_command zmx_resume_shell zmx_resume_shell_flag zmx_keepalive_shell zmx_keepalive_shell_login_flag\nunset ZMX_SESSION ZMX_SESSION_PREFIX\nif ! command -v zmx >/dev/null 2>&1; then\n  printf '\\''%s\\n'\\'' '\\''zmx was not found on PATH.'\\''\n  exit 127\nfi\nif zmx list --short 2>/dev/null | grep -F -x -- \"$zmx_session\" >/dev/null 2>&1; then\n  exec zmx attach \"$zmx_session\"\nfi\ncd \"$zmx_cwd\" || exit\nzmx_resume_launcher='\\''\nset +e\n\"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_command\"\nzmx_resume_status=$?\nif [ \"$zmx_resume_status\" -ne 0 ] && [ -n \"$zmx_resume_fallback_command\" ] && [ \"$zmx_resume_fallback_command\" != \"$zmx_resume_command\" ]; then\n  printf '\\''\"'\\''\"'\\''%s\\n'\\''\"'\\''\"'\\'' \"Exact resume failed; trying saved fallback resume command.\"\n  \"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_fallback_command\"\n  zmx_resume_status=$?\nfi\nif [ \"$zmx_resume_status\" -ne 0 ]; then\n  printf '\\''\"'\\''\"'\\''\\n%s\\n'\\''\"'\\''\"'\\'' \"Resume command exited with status $zmx_resume_status. Leaving this pane open for inspection.\"\n  if [ -n \"$zmx_keepalive_shell_login_flag\" ]; then\n    exec \"$zmx_keepalive_shell\" \"$zmx_keepalive_shell_login_flag\"\n  fi\n  exec \"$zmx_keepalive_shell\"\nfi\nexit 0\n'\\''\nexec zmx attach \"$zmx_session\" \"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_launcher\"\n'";
        assert_eq!(
            build_zmx_attach_or_resume_command_with(&session, &zsh_shell(), true),
            expected
        );
    }

    #[test]
    fn zmx_bootstrap_matches_node_output_linux() {
        let session = json!({
            "providerSessionName": "g-0713-2",
            "resumeCommand": "codex resume 'x'",
            "projectPath": "",
        });
        let shell = CliShellLaunch {
            command_flag: "-c".to_string(),
            executable: "/bin/sh".to_string(),
            login_flag: "".to_string(),
        };
        let expected = "/bin/sh -c '\nzmx_session='\\''g-0713-2'\\''\nzmx_resume_command='\\''codex resume '\\''\\'\\'''\\''x'\\''\\'\\'''\\'''\\''\nzmx_resume_fallback_command='\\'''\\''\nzmx_cwd='\\''.'\\''\nzmx_resume_shell='\\''/bin/sh'\\''\nzmx_resume_shell_flag='\\''-c'\\''\nzmx_keepalive_shell='\\''/bin/sh'\\''\nzmx_keepalive_shell_login_flag='\\'''\\''\nexport zmx_resume_command zmx_resume_fallback_command zmx_resume_shell zmx_resume_shell_flag zmx_keepalive_shell zmx_keepalive_shell_login_flag\nunset ZMX_SESSION ZMX_SESSION_PREFIX\nif ! command -v zmx >/dev/null 2>&1; then\n  printf '\\''%s\\n'\\'' '\\''zmx was not found on PATH.'\\''\n  exit 127\nfi\nif zmx list --short 2>/dev/null | grep -F -x -- \"$zmx_session\" >/dev/null 2>&1; then\n  exec zmx attach \"$zmx_session\"\nfi\ncd \"$zmx_cwd\" || exit\nzmx_resume_launcher='\\''\nset +e\n\"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_command\"\nzmx_resume_status=$?\nif [ \"$zmx_resume_status\" -ne 0 ] && [ -n \"$zmx_resume_fallback_command\" ] && [ \"$zmx_resume_fallback_command\" != \"$zmx_resume_command\" ]; then\n  printf '\\''\"'\\''\"'\\''%s\\n'\\''\"'\\''\"'\\'' \"Exact resume failed; trying saved fallback resume command.\"\n  \"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_fallback_command\"\n  zmx_resume_status=$?\nfi\nif [ \"$zmx_resume_status\" -ne 0 ]; then\n  printf '\\''\"'\\''\"'\\''\\n%s\\n'\\''\"'\\''\"'\\'' \"Resume command exited with status $zmx_resume_status. Leaving this pane open for inspection.\"\n  if [ -n \"$zmx_keepalive_shell_login_flag\" ]; then\n    exec \"$zmx_keepalive_shell\" \"$zmx_keepalive_shell_login_flag\"\n  fi\n  exec \"$zmx_keepalive_shell\"\nfi\nexit 0\n'\\''\nexec zmx attach \"$zmx_session\" \"$zmx_resume_shell\" \"$zmx_resume_shell_flag\" \"$zmx_resume_launcher\"\n'";
        assert_eq!(
            build_zmx_attach_or_resume_command_with(&session, &shell, false),
            expected
        );
    }

    #[test]
    fn shell_word_quotes_only_when_needed() {
        assert_eq!(shell_word("/bin/zsh"), "/bin/zsh");
        assert_eq!(shell_word("my shell"), "'my shell'");
        assert_eq!(shell_word(""), "''");
    }
}
