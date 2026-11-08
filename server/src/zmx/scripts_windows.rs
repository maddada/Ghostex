use super::scripts::{ZmxAttachCommandInput, ZmxRunCommandInput, ZmxShellProviderCommandInput};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, process::Command};

const PREFIX: &str = "ghostex-native-session:";
#[derive(Serialize, Deserialize)]
struct Invocation {
    program: String,
    args: Vec<String>,
    #[serde(default)]
    environment: HashMap<String, String>,
}

fn invocation(
    program: &str,
    args: Vec<String>,
    mut environment: HashMap<String, String>,
) -> String {
    environment.insert("WMX_DIR".into(), session_directory());
    format!(
        "{PREFIX}{}",
        serde_json::to_string(&Invocation {
            program: program.into(),
            args,
            environment
        })
        .expect("serialize native session invocation")
    )
}

/// The wmx command, its environment, and the creation flags it starts with.
///
/// CDXC:PlatformSupport 2026-10-06 WHY:
/// gxserver suppresses Windows error dialogs for the helpers it starts (`suppress_helper_error_dialogs`), and children inherit that error mode. `wmx start` launches the daemon whose PowerShell and agent run inside the user's session, so it gets CREATE_DEFAULT_ERROR_MODE: programs the user runs there keep Windows' normal error dialogs.
pub(super) fn process(
    script: &str,
) -> Result<Option<(Command, HashMap<String, String>, u32)>, String> {
    const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;
    let Some(encoded) = script.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let invocation: Invocation =
        serde_json::from_str(encoded).map_err(|error| error.to_string())?;
    // A session host start keeps a console of its own, as before; every other wmx helper shares gxserver's windowless console (`hidden_console_flags`).
    let flags = match invocation.args.first().map(String::as_str) {
        Some("start" | "start-encoded") => {
            crate::platform::process::CREATE_NO_WINDOW | CREATE_DEFAULT_ERROR_MODE
        }
        _ => crate::platform::process::hidden_console_flags(),
    };
    let mut command = Command::new(invocation.program);
    command.args(invocation.args);
    Ok(Some((command, invocation.environment, flags)))
}

/// The result of the wmx command `script` names, made in process when that command is one request to the session host (`wmx_client`); `None` when wmx has to run.
pub(super) fn answer_in_process(
    script: &str,
    options: &super::ZmxCommandOptions,
) -> Option<super::ZmxCommandResult> {
    let invocation: Invocation = serde_json::from_str(script.strip_prefix(PREFIX)?).ok()?;
    let directory = invocation.environment.get("WMX_DIR")?;
    super::wmx_client::answer(&invocation.args, directory, options)
}

fn simple(program: &str, operation: &str, name: &str) -> String {
    invocation(program, vec![operation.into(), name.into()], HashMap::new())
}

pub(crate) fn list_command(program: &str, short: bool) -> String {
    invocation(
        program,
        if short {
            vec!["list".into(), "--short".into()]
        } else {
            vec!["list".into()]
        },
        HashMap::new(),
    )
}

fn quote(text: &str) -> String {
    crate::platform::shell::powershell_single_quote(text)
}

pub(crate) fn build_zmx_attach_command(input: ZmxAttachCommandInput) -> String {
    format!(
        "$env:WMX_DIR={}; & {} attach --require-existing {}{}",
        quote(&session_directory()),
        quote(&input.zmx_executable_path),
        quote(&input.session_name),
        input
            .prompt_editor
            .as_deref()
            .map(|editor| format!(" --prompt-editor {}", quote(editor)))
            .unwrap_or_default()
    )
}
pub(crate) fn build_started_zmx_attach_command(input: ZmxAttachCommandInput) -> String {
    build_zmx_attach_command(input)
}
pub(crate) fn build_zmx_kill_command(name: &str, program: &str) -> String {
    simple(program, "kill", name)
}
pub(crate) fn build_zmx_history_command(name: &str, program: &str) -> String {
    simple(program, "history", name)
}
pub(crate) fn build_zmx_screen_capture_command(
    name: &str,
    program: &str,
    scrollback: u32,
    vt: bool,
) -> String {
    let mut args = vec![
        "history".into(),
        name.into(),
        "--scrollback".into(),
        scrollback.to_string(),
    ];
    if vt {
        args.push("--vt".into());
    }
    invocation(program, args, HashMap::new())
}
pub(crate) fn build_zmx_grid_command(name: &str, program: &str) -> String {
    simple(program, "grid", name)
}
pub(crate) fn build_zmx_send_command(name: &str, program: &str) -> String {
    simple(program, "send", name)
}
pub(crate) fn build_zmx_exists_command(name: &str, program: &str) -> String {
    simple(program, "exists", name)
}

pub(crate) fn build_zmx_run_command(input: ZmxRunCommandInput) -> String {
    let environment = input.launch_environment();
    start(
        &input.zmx_executable_path,
        &input.session_name,
        &input.cwd,
        Some(&input.startup_text),
        input.global_session_ref,
        input.gxserver_auth_token_file,
        input.gxserver_base_url,
        input.gxserver_protocol_version,
        environment,
    )
}
pub(crate) fn build_zmx_shell_provider_command(input: ZmxShellProviderCommandInput) -> String {
    start(
        &input.zmx_executable_path,
        &input.session_name,
        &input.cwd,
        None,
        input.global_session_ref,
        input.gxserver_auth_token_file,
        input.gxserver_base_url,
        input.gxserver_protocol_version,
        super::scripts::session_token_environment(input.session_token.as_deref()),
    )
}

/// CDXC:PromptEditor 2026-09-23 WHY:
/// Apply the editor handshake after the PowerShell profile and pin it to this server's sibling CLI; bare `ghostex` selected an obsolete package from PATH even after the desktop and editor were updated.
#[allow(clippy::too_many_arguments)]
fn start(
    program: &str,
    name: &str,
    cwd: &str,
    startup: Option<&str>,
    global_ref: Option<String>,
    token_file: Option<String>,
    base_url: Option<String>,
    version: Option<u64>,
    coordinator_environment: Vec<(&'static str, String)>,
) -> String {
    let mut environment = HashMap::new();
    // A coordinator's or thread's markers (coordinators/launch_env.rs).
    for (key, value) in coordinator_environment {
        environment.insert(key.to_string(), value);
    }
    for (key, value) in [
        ("GHOSTEX_GLOBAL_SESSION_REF", global_ref),
        ("GHOSTEX_GXSERVER_AUTH_TOKEN_FILE", token_file),
        ("GHOSTEX_GXSERVER_BASE_URL", base_url),
        (
            "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
            version.map(|value| value.to_string()),
        ),
        ("GHOSTEX_SESSION_ID", Some(name.to_string())),
        ("GHOSTEX_ZMX_BIN", Some(program.to_string())),
        (
            super::PROMPT_HANDSHAKE_DIR_ENV,
            Some(super::prompt_handshake_dir().to_string_lossy().into_owned()),
        ),
        // CDXC:AgentScreenDetection 2026-10-07 WHY: sessions advertise TERM_PROGRAM=ghostty, so Empryo draws its chat avatars as Kitty image placeholders (U+10EEEE plus row and column diacritics). wmx's screen model does not place those cells the way Empryo does, so every capture of a window showing them came back shifted and overwritten (seen live 2026-10-07: "Empr̎pryo" rows, an input box missing its right border), and Empryo's chat could find no input box and held every send. Empryo is told not to draw them in wmx sessions until wmx models the placeholders. SEE-ALSO: .dependencies/wmx (its VT screen).
        ("EMPRYO_NO_KITTY_IMAGES", Some("1".to_string())),
        // Claude's task list while the Session details panel asks for it (session_details/gate.rs).
        (
            "CLAUDE_CODE_ENABLE_TODO_TOOLS",
            crate::session_details::claude_task_tools_environment(
                &crate::paths::get_gxserver_paths(None),
            )
            .map(|(_, value)| value.to_string()),
        ),
    ] {
        if let Some(value) = value {
            environment.insert(key.into(), value);
        }
    }
    let cli = std::env::current_exe()
        .expect("resolve running gxserver for the session editor")
        .with_file_name("ghostex.exe");
    let editor = format!("\"{}\" prompt-editor", cli.to_string_lossy());
    // The profile runs after ConPTY applies launch.cwd and may change location.
    // Restore the selected project before any agent command can run.
    let startup = format!(
        "Set-Location -LiteralPath {} -ErrorAction Stop; {}{}$env:GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL=$env:VISUAL; $env:GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR=$env:EDITOR; $env:VISUAL={}; $env:EDITOR=$env:VISUAL; $env:GHOSTEX_PROMPT_EDITING_ENABLED='1'; {}",
        quote(cwd),
        portable_cli_first_on_path(&cli),
        super::codex_wrapper::wmx_codex_wrapper_path_command(),
        quote(&editor),
        agent_without_powershell7_modules(startup.unwrap_or_default().trim_end_matches(['\r', '\n']))
    );
    let startup = startup_from_environment(&startup, &mut environment);
    let launch = json!({"name": name, "cwd": cwd,
        "shell": crate::platform::shell::command_shell().executable,
        "startup": startup});
    let encoded =
        STANDARD.encode(serde_json::to_vec(&launch).expect("serialize native session launch"));
    invocation(program, vec!["start-encoded".into(), encoded], environment)
}

/// CDXC:PlatformSupport 2026-10-17 WHY:
/// A portable copy does not install the public `ghostex` wrappers, which belong to an installed Ghostex (ghostex_paths::portable), so `ghostex` typed in its sessions, by the user or an agent, would reach the installed app's gxserver. Its sessions put this gxserver's own CLI folder first on PATH, after the profile has run. An installed Ghostex's sessions keep the PATH their profile gives them.
fn portable_cli_first_on_path(cli: &std::path::Path) -> String {
    if ghostex_paths::portable::current().is_none() {
        return String::new();
    }
    let Some(directory) = cli.parent() else {
        return String::new();
    };
    format!(
        "$env:Path={} + ';' + $env:Path; ",
        quote(&directory.to_string_lossy())
    )
}

/// Runs the agent's startup command with PowerShell 7's own module folder out of `PSModulePath`, then puts the shell's value back.
///
/// CDXC:PlatformSupport 2026-10-08 WHY:
/// pwsh 7 writes its `$PSHOME\Modules` folder into `PSModulePath` and every child inherits it. A Windows PowerShell 5.1 child (`powershell.exe`) started by the agent itself, for example Codex's "Update" running `powershell -c 'irm https://chatgpt.com/codex/install.ps1 | iex'`, then loads that folder's Core-only `Microsoft.PowerShell.Utility` 7.0 before its own 3.1 and loses cmdlets such as Get-FileHash ("iex : The term 'Get-FileHash' is not recognized"). pwsh repairs the value only for 5.1 processes it starts itself, so the same command typed at the prompt worked. Reproduced from a Node parent in a wmx session: only the `$PSHOME\Modules` entry breaks 5.1; `Documents\PowerShell\Modules` and `Program Files\PowerShell\Modules` are harmless.
/// The value cannot be fixed for the shell itself: with the entry removed, or merely moved behind the Windows PowerShell folders, pwsh 7 resolves PSReadLine 2.0 and loads Microsoft.PowerShell.Archive through the 5.1 compatibility session. So only the agent's command line loses it, and the shell's value returns when the agent exits. A child `pwsh` re-adds its own folder at startup, so agents that run pwsh tools are unaffected. The guard on `PSEdition` keeps this a no-op when the session shell is Windows PowerShell 5.1, where `$PSHOME\Modules` is 5.1's own folder.
fn agent_without_powershell7_modules(command: &str) -> String {
    if command.is_empty() {
        return String::new();
    }
    format!(
        "if ($PSVersionTable.PSEdition -eq 'Core') {{ $ghostexPsModulePath=$env:PSModulePath; $ghostexPwshModules=(Join-Path $PSHOME 'Modules').TrimEnd('\\'); $env:PSModulePath=(($env:PSModulePath -split ';') | Where-Object {{ $_ -and $_.TrimEnd('\\') -ne $ghostexPwshModules }}) -join ';' }}; try {{ {command}\n}} finally {{ if ($null -ne $ghostexPsModulePath) {{ $env:PSModulePath=$ghostexPsModulePath; Remove-Variable ghostexPsModulePath, ghostexPwshModules }} }}"
    )
}

/// CDXC:PlatformSupport 2026-09-25 WHY:
/// Resume lookup can contain the full first prompt. Putting it in wmx's launch argument and then PowerShell's UTF-16 EncodedCommand exceeded Windows' command-line limit and prevented session attach. Transport the UTF-8 payload in bounded environment values, consumed before the agent starts, so neither command line grows with the prompt.
fn startup_from_environment(startup: &str, environment: &mut HashMap<String, String>) -> String {
    let encoded = STANDARD.encode(startup.as_bytes());
    let mut count = 0;
    for (index, chunk) in encoded.as_bytes().chunks(8192).enumerate() {
        environment.insert(
            format!("GHOSTEX_NATIVE_STARTUP_{index}"),
            String::from_utf8(chunk.to_vec()).expect("base64 is ASCII"),
        );
        count += 1;
    }
    format!(
        ". (& {{ $payload = -join @(for ($i = 0; $i -lt {count}; $i++) {{ \
         $key = 'GHOSTEX_NATIVE_STARTUP_' + $i; \
         [Environment]::GetEnvironmentVariable($key, 'Process'); \
         Remove-Item -LiteralPath ('Env:' + $key) -ErrorAction Stop \
         }}); [ScriptBlock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($payload))) }})"
    )
}

pub(crate) fn process_snapshot_command(program: &str) -> String {
    invocation(program, vec!["process-snapshot".into()], HashMap::new())
}

/// CDXC:PlatformSupport 2026-09-14 WHY:
/// Keep the existing endpoint directory when migrating to wmx so running native sessions remain attachable without restarting their agents.
pub(crate) fn session_directory() -> String {
    ghostex_paths::GhostexPaths::resolve()
        .runtime_dir
        .join("windows-sessions")
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn version_command(program: &str) -> String {
    invocation(program, vec!["version".into()], HashMap::new())
}
pub(crate) fn inspect_command(program: &str, name: &str) -> String {
    simple(program, "inspect", name)
}
pub(crate) fn force_kill_command(program: &str, name: &str) -> String {
    invocation(
        program,
        vec!["kill".into(), "--force".into(), name.into()],
        HashMap::new(),
    )
}
