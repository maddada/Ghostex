use super::*;
use crate::ghostex_cli::args::js_number;

const DEFAULT_PORT: f64 = 58743.0;
const DEV_PORT: f64 = 58742.0;

/// bridgePortFromFlags.
pub(super) fn bridge_port_from_flags(flags: &Flags) -> f64 {
    if let Some(port) = flags.number("port") {
        return port;
    }
    if let Ok(port) = std::env::var("GHOSTEX_CLI_PORT") {
        if let Some(port) = js_number(&port) {
            return port;
        }
    }
    if std::env::var("GHOSTEX_APP_VARIANT").as_deref() == Ok("dev") {
        DEV_PORT
    } else {
        DEFAULT_PORT
    }
}

/// readBridgeAuthToken.
pub(super) fn read_bridge_auth_token(flags: &Flags) -> CliResult<String> {
    /*
    CDXC:ServerApi 2026-05-15-18:25 (ported): CLI commands read
    the per-launch token that the app writes under resolved Ghostex state storage.
    */
    let explicit_token = flags
        .text("token")
        .or_else(|| flags.text("bridgeToken"))
        .or_else(|| std::env::var("GHOSTEX_BRIDGE_TOKEN").ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    if !explicit_token.is_empty() {
        return Ok(explicit_token);
    }
    let bridge_token_path = rpc::ghostex_home().join("cli").join("bridge-token");
    let token = std::fs::read_to_string(&bridge_token_path)
        .unwrap_or_default()
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(CliError::Other(format!(
            "Could not read Ghostex bridge token at {}. Is Ghostex running?",
            bridge_token_path.display()
        )));
    }
    Ok(token)
}

/// floatingEditorEnvironment.
pub(super) fn floating_editor_environment() -> Value {
    let env_or =
        |key: &str, fallback: &str| std::env::var(key).unwrap_or_else(|_| fallback.to_string());
    let mut environment = Map::new();
    environment.insert(
        "HOME".to_string(),
        json!(std::env::var("HOME")
            .unwrap_or_else(|_| rpc::home_dir().to_string_lossy().into_owned())),
    );
    environment.insert("LANG".to_string(), json!(env_or("LANG", "en_US.UTF-8")));
    environment.insert(
        "PATH".to_string(),
        json!(env_or(
            "PATH",
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        )),
    );
    environment.insert("SHELL".to_string(), json!(env_or("SHELL", "/bin/zsh")));
    environment.insert("TERM".to_string(), json!(env_or("TERM", "xterm-256color")));
    environment.insert("USER".to_string(), json!(env_or("USER", "")));
    environment.insert("GHOSTEX_FLOATING_EDITOR".to_string(), json!("1"));
    if let Ok(variant) = std::env::var("GHOSTEX_APP_VARIANT") {
        environment.insert("GHOSTEX_APP_VARIANT".to_string(), json!(variant));
    }
    Value::Object(environment)
}

/// floatingEditorWrapperScript.
pub(super) fn floating_editor_wrapper_script(
    command_args: &[String],
    cwd: &str,
    status_file: &str,
    log_path: &str,
) -> String {
    let command = command_args
        .iter()
        .map(|arg| cli_shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let dirname = |path: &str| {
        Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string())
    };
    format!(
        "#!/bin/zsh\nset +e\nmkdir -p {status_dir} {log_dir} 2>/dev/null\nprintf 'started\\n' > {status}\n{{\n  printf '[%s] child.start cwd=%s command=%s\\n' \"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\" {cwd_quoted} {command_quoted}\n}} >> {log} 2>/dev/null\ncd {cwd_quoted} || {{\n  _ghostex_status=$?\n  printf 'exit:%s\\n' \"$_ghostex_status\" >> {status}\n  exit \"$_ghostex_status\"\n}}\n{command}\n_ghostex_status=$?\n{{\n  printf '[%s] child.exit status=%s\\n' \"$(date -u '+%Y-%m-%dT%H:%M:%SZ')\" \"$_ghostex_status\"\n}} >> {log} 2>/dev/null\nprintf 'exit:%s\\n' \"$_ghostex_status\" >> {status}\nexit \"$_ghostex_status\"\n",
        status_dir = cli_shell_quote(&dirname(status_file)),
        log_dir = cli_shell_quote(&dirname(log_path)),
        status = cli_shell_quote(status_file),
        cwd_quoted = cli_shell_quote(cwd),
        command_quoted = cli_shell_quote(&command),
        log = cli_shell_quote(log_path),
        command = command,
    )
}

/// resolveExecutable (command -v through the interactive shell).
pub(super) fn resolve_executable(command: &str) -> CliResult<String> {
    if command.contains('/') {
        return Ok(command.to_string());
    }
    let shell = resolve_cli_interactive_shell_launch();
    let output = Command::new(&shell.executable)
        .arg(shell.command_flag)
        .arg(format!("command -v -- {}", cli_shell_quote(command)))
        .output()
        .map_err(|error| CliError::Other(error.to_string()))?;
    if !output.status.success() {
        return Err(CliError::Other(format!(
            "Command failed: command -v -- {}",
            cli_shell_quote(command)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout
        .trim()
        .split(['\r', '\n'])
        .next()
        .unwrap_or("")
        .to_string();
    Ok(if first_line.is_empty() {
        command.to_string()
    } else {
        first_line
    })
}

pub(super) struct ShellLaunch {
    pub executable: String,
    pub command_flag: &'static str,
}

/// resolveCliInteractiveShellLaunch (macOS pinned to zsh; POSIX shells
/// elsewhere).
pub(super) fn resolve_cli_interactive_shell_launch() -> ShellLaunch {
    if cfg!(target_os = "macos") {
        return ShellLaunch {
            executable: "/bin/zsh".to_string(),
            command_flag: "-lc",
        };
    }
    let mut candidates: Vec<String> = Vec::new();
    let shell = std::env::var("SHELL")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !shell.is_empty() && is_supported_cli_posix_shell(&shell) {
        candidates.push(shell);
    }
    for fallback in ["/bin/bash", "/usr/bin/bash", "/bin/sh", "/usr/bin/sh"] {
        if !candidates.iter().any(|candidate| candidate == fallback) {
            candidates.push(fallback.to_string());
        }
    }
    let executable = candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .cloned()
        .or_else(|| candidates.first().cloned())
        .unwrap_or_else(|| "/bin/sh".to_string());
    let command_flag = if matches!(shell_basename(&executable).as_str(), "bash" | "zsh") {
        "-lc"
    } else {
        "-c"
    };
    ShellLaunch {
        executable,
        command_flag,
    }
}

fn is_supported_cli_posix_shell(shell_path: &str) -> bool {
    matches!(
        shell_basename(shell_path).as_str(),
        "ash" | "bash" | "dash" | "ksh" | "mksh" | "sh" | "zsh"
    )
}

fn shell_basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_lowercase()
}

/// waitForStatus (status-file polling loop).
pub(super) fn wait_for_status(
    status_file: &Path,
    predicate: &dyn Fn(&str) -> bool,
    timeout_ms: u64,
) -> CliResult<String> {
    let started_at = Instant::now();
    loop {
        let status = std::fs::read_to_string(status_file).unwrap_or_default();
        if predicate(&status) {
            return Ok(status);
        }
        if timeout_ms > 0 && started_at.elapsed().as_millis() as u64 > timeout_ms {
            return Err(CliError::Other(format!(
                "Timed out waiting for floating editor status at {}.",
                status_file.display()
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
