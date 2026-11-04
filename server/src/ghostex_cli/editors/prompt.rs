use super::*;

pub fn prompt_editor_command(args: &[String]) -> CliResult<()> {
    /*
    CDXC:PromptEditor 2026-06-30-03:11 (ported):
    Ctrl+G fallback is the machine's editor, not gte. Preserve Monaco only for
    macOS app clients or zmx leaders that explicitly advertise Monaco; every
    other context runs the first non-Ghostex editor from the preserved
    provider environment, VISUAL, EDITOR, then vi.
    */
    let selection_started_at = Instant::now();
    let parsed = parse_args(args);
    let Some(file_path) = parsed.rest.iter().find(|arg| !arg.trim().is_empty()) else {
        return Err(CliError::Other(
            "Usage: ghostex prompt-editor <file>".to_string(),
        ));
    };

    let cwd = js_path_resolve(&parsed.flags.text("cwd").unwrap_or_else(current_dir_string));
    let resolved_file_path = js_path_resolve_from(&cwd, file_path)
        .to_string_lossy()
        .into_owned();
    let backend = prompt_editor_backend_from_environment();
    let capability_started_at = Instant::now();
    let client_capability = zmx_prompt_editor_capability();
    let capability_duration_ms = elapsed_ms(capability_started_at);
    let selection =
        select_prompt_editor_command(&backend, client_capability.as_deref(), &resolved_file_path);
    let originating_session_id = prompt_editor_originating_session_id_from_environment();
    let selection_duration_ms = elapsed_ms(selection_started_at);

    append_floating_editor_log(json!({
        "backend": backend,
        "command": selection.command_args.join(" "),
        "cwd": cwd.to_string_lossy(),
        "event": "cli.prompt_editor_select",
        "globalSessionRef": env_or_empty("GHOSTEX_GLOBAL_SESSION_REF"),
        "gxserverBaseUrl": env_or_empty("GHOSTEX_GXSERVER_BASE_URL"),
        "macosAppClient": is_macos_app_prompt_editor_client(client_capability.as_deref()),
        "originatingSessionId": originating_session_id.clone().unwrap_or_default(),
        "promptEditorClientCapability": client_capability.clone().unwrap_or_default(),
    }));

    /*
    CDXC:SavedPrompts 2026-07-29:
    The GPUI "Stash Prompt" agent action writes a one-shot marker for the
    session and sends Ctrl+G. When this invocation finds a fresh marker, the
    file already holds the composer text the agent CLI wrote for $EDITOR, so
    stash it and exit without presenting any editor. Clearing the file on a
    durable stash is the visible result: the agent CLI reads the emptied file
    back and the composer text has moved into the stash.
    */
    if consume_prompt_stash_request(originating_session_id.as_deref(), &resolved_file_path) {
        crate::ghostex_cli::set_exit_code(0);
        return Ok(());
    }

    if selection.kind == "monaco" {
        let trace = json!({
            "backend": backend,
            "capabilityDurationMs": capability_duration_ms,
            "clientCapability": client_capability.unwrap_or_default(),
            "hasGlobalSessionRef": !env_or_empty("GHOSTEX_GLOBAL_SESSION_REF").is_empty(),
            "hasOriginatingSessionId": originating_session_id.is_some(),
            "selectionDurationMs": selection_duration_ms,
            "selectionKind": selection.kind,
        });
        let original = match std::fs::read(&resolved_file_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(CliError::Other(format!(
                    "Could not read the prompt before editing: {error}"
                )));
            }
        };
        let status = floating_monaco_editor_command_with_trace(args, &trace)?;
        finish_external_prompt_edit(status.as_deref(), &resolved_file_path, original.as_deref())?;
        return Ok(());
    }
    if selection.kind == "code-server" {
        return run_code_server_prompt_editor(&selection.command_args, &cwd);
    }
    run_editor_inline(&selection.command_args, &cwd)
}

/// CDXC:PromptEditor 2026-09-23 WHY:
/// Agent CLIs interpret a nonzero EDITOR exit as a launch failure, even after the user intentionally cancels.
/// The EDITOR adapter restores the original input and succeeds on explicit cancellation; the direct floating-editor command retains its cancellation status for other consumers.
fn finish_external_prompt_edit(
    status: Option<&str>,
    path: &str,
    original: Option<&[u8]>,
) -> CliResult<()> {
    if status == Some("cancelled") {
        let restored = if let Some(original) = original {
            if std::fs::read(path).ok().as_deref() == Some(original) {
                Ok(())
            } else {
                std::fs::write(path, original)
            }
        } else {
            match std::fs::remove_file(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                result => result,
            }
        };
        if let Err(error) = restored {
            return Err(CliError::Other(format!(
                "Could not restore the cancelled prompt: {error}"
            )));
        }
        crate::ghostex_cli::set_exit_code(0);
    }
    Ok(())
}

pub(super) fn prompt_editor_backend_from_environment() -> String {
    let backend = env_or_empty("GHOSTEX_PROMPT_EDITOR_BACKEND")
        .trim()
        .to_string();
    if backend == "monaco" || backend == "custom" {
        return backend;
    }
    // "gte" and GHOSTEX_RICH_PROMPT_EDITING_WITH_GTE both collapse to inherit.
    "inherit".to_string()
}

pub(super) fn zmx_prompt_editor_capability() -> Option<String> {
    if env_or_empty("ZMX_SESSION").trim().is_empty() {
        return None;
    }
    /*
    CDXC:PromptEditor 2026-06-07-08:09 (ported): zmx sessions without an
    explicit GHOSTEX_ZMX_BIN stay terminal-native instead of probing PATH.
    */
    let zmx_command = env_or_empty("GHOSTEX_ZMX_BIN").trim().to_string();
    if zmx_command.is_empty() {
        return Some("editor".to_string());
    }
    let capability = run_zmx_capability_probe(&zmx_command);
    match capability.as_deref() {
        Some("monaco") | Some("code-server") | Some("editor") | Some("gte") => capability,
        _ => Some("editor".to_string()),
    }
}

/// execFileAsync(zmx, ["prompt-editor-capability"], { timeout: 750 }).
pub(super) fn run_zmx_capability_probe(command: &str) -> Option<String> {
    let mut probe = crate::platform::process::background_command(command);
    let mut child = probe
        .arg("prompt-editor-capability")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_millis(750);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut output = String::new();
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return Some(output.trim().to_string());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
}

pub(super) fn is_macos_app_prompt_editor_client(client_capability: Option<&str>) -> bool {
    is_macos_app_prompt_editor_client_with(client_capability, &|key| std::env::var(key).ok())
}

pub(super) fn is_macos_app_prompt_editor_client_with(
    client_capability: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> bool {
    if let Some(capability) = client_capability.filter(|value| !value.is_empty()) {
        return capability == "monaco";
    }
    env("GHOSTEX_PROMPT_EDITOR_CLIENT").as_deref() == Some("macos-app")
}

pub(super) struct PromptEditorSelection {
    pub(super) command_args: Vec<String>,
    pub(super) kind: &'static str,
}

pub(super) fn select_prompt_editor_command(
    backend: &str,
    client_capability: Option<&str>,
    file_path: &str,
) -> PromptEditorSelection {
    select_prompt_editor_command_with(
        backend,
        client_capability,
        file_path,
        &|key| std::env::var(key).ok(),
        &|| resolve_ghostex_editor_executable().is_some(),
        &mut |message| eprintln!("{message}"),
    )
}

pub(super) fn select_prompt_editor_command_with(
    backend: &str,
    client_capability: Option<&str>,
    file_path: &str,
    env: &dyn Fn(&str) -> Option<String>,
    ghostex_editor_available: &dyn Fn() -> bool,
    warn: &mut dyn FnMut(&str),
) -> PromptEditorSelection {
    let capability = client_capability.filter(|value| !value.is_empty());
    if capability == Some("code-server") {
        return PromptEditorSelection {
            command_args: code_server_prompt_editor_command(file_path),
            kind: "code-server",
        };
    }
    if backend == "custom" {
        let custom_command = env("GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "code --wait".to_string());
        return PromptEditorSelection {
            command_args: machine_editor_args(&custom_command, file_path),
            kind: "custom",
        };
    }
    if (backend == "monaco" || capability == Some("monaco"))
        && is_macos_app_prompt_editor_client_with(capability, env)
    {
        if ghostex_editor_available() {
            return PromptEditorSelection {
                command_args: vec![
                    "ghostex".to_string(),
                    "floating-monaco-editor".to_string(),
                    file_path.to_string(),
                ],
                kind: "monaco",
            };
        }
        warn(&ghostex_editor_unavailable_message(None));
    }
    let editor_command = machine_prompt_editor_command_with(env);
    PromptEditorSelection {
        command_args: machine_editor_args(&editor_command, file_path),
        kind: "editor",
    }
}

/// The native editor the app installed on demand: `code-server\<version folder>\windows-native-<arch>` in the component store, complete once its install marker exists.
#[cfg(windows)]
fn installed_native_code_server() -> Option<PathBuf> {
    let platform = if cfg!(target_arch = "aarch64") {
        "windows-native-arm64"
    } else {
        "windows-native-x64"
    };
    let versions =
        std::fs::read_dir(ghostex_paths::windows_component_store_dir()?.join("code-server"))
            .ok()?;
    versions
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path().join(platform))
        .find(|path| path.join(".ghostex-component.json").is_file())
}

/// CDXC:PromptEditor 2026-10-20 WHY:
/// The managed Windows CLI lives under Data/gxserver, apart from the native Code editor. Release builds install that editor on demand into the component store (`ghostex_paths::windows_component_store_dir`); development builds still carry it beside the app. Try the app folder the app recorded (the install that last ran), then the per-user install the release installer and the local start both use (`%LOCALAPPDATA%\Ghostex\current`), then a Program Files install, then the component store, matching the remote Code launcher. Supersedes the 2026-10-07 order, which had no component store.
/// SEE-ALSO: apps/desktop/src/app/helpers/remote/windows_code.rs, ghostex_paths::GhostexPaths::windows_app_dir_file, component_version_dir_name in apps/desktop/src/component_store/install_files.rs.
pub(super) fn code_server_prompt_editor_command(file_path: &str) -> Vec<String> {
    let code_root = rpc::ghostex_data_home().join("code-server");
    let package = code_root.join("package");
    #[cfg(windows)]
    let package = [
        std::fs::read_to_string(rpc::storage_paths().windows_app_dir_file())
            .ok()
            .filter(|app_dir| !app_dir.trim().is_empty())
            .map(|app_dir| PathBuf::from(app_dir.trim()).join("code-server")),
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Ghostex/current/code-server")),
        std::env::var_os("ProgramW6432")
            .filter(|root| !root.is_empty())
            .or_else(|| std::env::var_os("ProgramFiles"))
            .map(PathBuf::from)
            .map(|root| root.join("Ghostex/code-server")),
        std::env::current_exe().ok().and_then(|exe| {
            exe.parent()?
                .parent()?
                .parent()
                .map(|app| app.join("code-server"))
        }),
        installed_native_code_server(),
        Some(package.clone()),
    ]
    .into_iter()
    .flatten()
    .find(|path| {
        path.join("lib/node.exe").is_file()
            && path.join("out/node/vscodeSocket.js").is_file()
            && path.join("lib/vscode/out/server-main.js").is_file()
    })
    .unwrap_or(package);
    let user_data = code_root.join("runtime/user-data");
    #[cfg(not(windows))]
    let socket = user_data.join("code-server-ipc.sock");
    #[cfg(windows)]
    let socket = {
        use sha2::{Digest, Sha256};
        let identity = user_data
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        PathBuf::from(format!(
            r"\\.\pipe\ghostex-code-{}",
            hex::encode(Sha256::digest(identity.as_bytes()))
        ))
    };
    vec![
        package
            .join(if cfg!(windows) {
                "lib/node.exe"
            } else {
                "lib/node"
            })
            .to_string_lossy()
            .into_owned(),
        "-e".to_string(),
        include_str!("code_server_prompt_editor.cjs").to_string(),
        package
            .join("out/node/vscodeSocket.js")
            .to_string_lossy()
            .into_owned(),
        socket.to_string_lossy().into_owned(),
        file_path.to_string(),
    ]
}

pub(super) fn run_code_server_prompt_editor(command_args: &[String], cwd: &Path) -> CliResult<()> {
    let mut command_args = command_args.to_vec();
    if prompt_editor_diagnostic_logging_enabled() {
        command_args.push(floating_editor_log_path().to_string_lossy().into_owned());
    }
    let node = command_args.first().map(Path::new);
    let session_manager = command_args.get(3).map(Path::new);
    let session_socket = command_args.get(4).map(Path::new);
    if !node.is_some_and(|path| path.to_str().is_some_and(is_executable_file))
        || !session_manager.is_some_and(|path| path.is_file())
        || !cwd.is_dir()
    {
        return Err(CliError::Other(
            "Remote Ghostex Code editor is unavailable for this session.".to_string(),
        ));
    }
    let Some(session_socket) = session_socket else {
        return Err(CliError::Other(
            "Remote Ghostex Code editor target is invalid.".to_string(),
        ));
    };
    #[cfg(windows)]
    {
        // The editor itself connects to the named pipe; filesystem existence is not a pipe readiness check.
        let _ = session_socket;
        return run_editor_inline(&command_args, cwd);
    }
    #[cfg(not(windows))]
    let deadline = Instant::now() + Duration::from_secs(10);
    #[cfg(not(windows))]
    while !session_socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    #[cfg(not(windows))]
    if !session_socket.exists() {
        return Err(CliError::Other(
            "Remote Ghostex Code editor did not become ready for this session.".to_string(),
        ));
    }
    #[cfg(not(windows))]
    run_editor_inline(&command_args, cwd)
}

pub(super) fn machine_prompt_editor_command_from_environment() -> String {
    machine_prompt_editor_command_with(&|key| std::env::var(key).ok())
}

pub(super) fn machine_prompt_editor_command_with(env: &dyn Fn(&str) -> Option<String>) -> String {
    [
        "GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL",
        "GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR",
        "VISUAL",
        "EDITOR",
    ]
    .iter()
    .filter_map(|key| env(key))
    .filter_map(|value| normalized_environment_string(&value))
    .find(|command| is_usable_machine_editor_command(command))
    .unwrap_or_else(|| if cfg!(windows) { "notepad.exe" } else { "vi" }.to_string())
}

pub(super) fn is_usable_machine_editor_command(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }
    !is_ghostex_prompt_editor_command(command)
}

pub(super) fn is_ghostex_prompt_editor_command(command: &str) -> bool {
    let trimmed = command.trim();
    let (executable, arguments) = match trimmed.chars().next() {
        Some(quote @ ('\'' | '"')) => trimmed[1..]
            .split_once(quote)
            .unwrap_or((&trimmed[1..], "")),
        _ => trimmed
            .split_once(char::is_whitespace)
            .unwrap_or((trimmed, "")),
    };
    let executable_name = executable.rsplit(['/', '\\']).next().unwrap_or("");
    executable_name == "prompt-editor"
        || (matches!(executable_name, "ghostex" | "ghostex.exe")
            && arguments.split_whitespace().next() == Some("prompt-editor"))
        || trimmed.contains("ghostex prompt-editor")
        || (trimmed.contains("ghostex-cli.mjs") && trimmed.contains("prompt-editor"))
        || trimmed.contains("floating-monaco-editor")
        || command.contains("floating-editor -- gte")
}

pub(super) fn prompt_editor_originating_session_id_from_environment() -> Option<String> {
    /*
    CDXC:PromptEditor 2026-06-09-21:50 (ported): derive the native P:G focus
    id from the per-session gxserver S:P:G ref before falling back to the
    legacy native env key for older direct terminals.
    */
    native_focus_session_id_from_global_session_ref(&env_or_empty("GHOSTEX_GLOBAL_SESSION_REF"))
        .or_else(|| normalized_environment_string(&env_or_empty("GHOSTEX_NATIVE_SESSION_ID")))
}

pub(super) fn native_focus_session_id_from_global_session_ref(
    global_session_ref: &str,
) -> Option<String> {
    let parts: Vec<&str> = global_session_ref.trim().split(':').collect();
    if parts.len() == 3
        && matches_session_ref_part(parts[0], b'S', 1)
        && matches_session_ref_part(parts[1], b'P', 3)
        && matches_session_ref_part(parts[2], b'G', 3)
    {
        return Some(format!("{}:{}", parts[1], parts[2]));
    }
    None
}

/// ^X[0-9][a-z0-9]{tail_len}$ for the S/P/G global session ref parts.
pub(super) fn matches_session_ref_part(part: &str, prefix: u8, tail_len: usize) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 + tail_len
        && bytes[0] == prefix
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// floating-monaco-editor (lines 3509-3712).

pub(super) fn machine_editor_args(command: &str, file_path: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let shell = crate::platform::shell::command_shell();
        let script = format!(
            "& {command} '{}'; exit $LASTEXITCODE",
            crate::platform::shell::powershell_single_quoted_content(file_path)
        );
        let mut args = vec![shell.executable.clone()];
        args.extend(shell.script_args(&script));
        args
    }
    #[cfg(not(windows))]
    {
        let shell = crate::platform::shell::command_shell();
        let mut args = vec![shell.executable.clone()];
        args.extend(shell.script_args(&format!("exec {command} \"$@\"")));
        args.extend(["ghostex-prompt-editor".into(), file_path.into()]);
        args
    }
}
