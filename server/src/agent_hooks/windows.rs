use serde_json::{json, Value};
use std::{io::Read, path::Path, sync::mpsc, time::Duration};

/// CDXC:AgentHooks 2026-09-14 WHY:
/// Windows command hooks must read JSON from stdin directly; passing it through Windows PowerShell 5.1 native argv strips JSON quotes.
/// Installation still uses the existing explicit install and Codex trust flow.
///
/// CDXC:AgentHooks 2026-10-09 WHY:
/// Claude Code and Grok use PowerShell or Git Bash and take the statusline's space-free executable and single-quoted arguments. PowerShell startup exceeded these hooks' budgets even outside Ghostex.
pub(crate) fn command(agent: &str, notify_path: &Path) -> String {
    let executable = std::env::current_exe().unwrap_or_default();
    let notify = notify_path.to_string_lossy();
    let quote = crate::platform::shell::powershell_single_quote;
    if agent == "codex" {
        return codex_command(&executable, notify_path, quote);
    }
    if agent == "hermes-agent" {
        return hermes_command(&executable, notify_path);
    }
    if let Some(executable) = bare_command_path(&executable)
        .filter(|_| matches!(agent, "claude" | "grok") && !notify.contains('\''))
    {
        return format!("{executable} agent-hook-notify-native '{notify}' '{agent}'");
    }
    if agent == "antigravity" {
        return antigravity_command(&executable, notify_path, quote);
    }
    format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -Command \"& {} agent-hook-notify-native {} {}\"",
        quote(&executable.to_string_lossy()),
        quote(&notify_path.to_string_lossy()),
        quote(agent)
    )
}

/// CDXC:AgentHooks 2026-10-11 WHY:
/// Antigravity CLI (agy 1.3.3) runs a Windows hook as `cmd /c "<command>"` with the command Go-escaped as one argument, so every double quote in it reaches cmd.exe as `\"`. The PowerShell wrapper's `-Command "& '…'"` therefore arrived as a quoted string, which PowerShell printed instead of running: no Antigravity hook ever reached gxserver (no conversation id, so the chat never found the transcript and a wake started a new conversation), and agy failed every tool call on the PreToolUse hook's unparseable output. The command carries no double quote: gxserver and the notify script by their space-free short paths, or, when a path has no short form, PowerShell's `-Command` as the rest of the line with cmd's `&` escaped as `^&` (PowerShell rejoins the arguments, so single-quoted paths keep their spaces).
fn antigravity_command(
    executable: &Path,
    notify_path: &Path,
    quote: impl Fn(&str) -> String,
) -> String {
    if let (Some(executable), Some(notify)) = (cmd_word(executable), cmd_word(notify_path)) {
        return format!("{executable} agent-hook-notify-native {notify} antigravity");
    }
    format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -Command ^& {} agent-hook-notify-native {} 'antigravity'",
        quote(&executable.to_string_lossy()),
        quote(&notify_path.to_string_lossy())
    )
}

/// CDXC:AgentHooks 2026-10-11 WHY:
/// Codex CLI (0.162.1) runs a Windows hook as `pwsh -NoProfile -Command <command>`, not through cmd.exe. The cmd-only `(if not defined … exit /b 0) & "…"` command installed on 2026-10-09 was a PowerShell parse error, so every Codex hook showed "Hook failed: hook exited with code 1" and none reached gxserver (no hook activity, transcript path or conversation id; only the footer status line still named the conversation). The command is gxserver and the notify script by their space-free short paths, which pwsh, Windows PowerShell and cmd.exe all run as written; gxserver's own outside-Ghostex check answers with the canned response, so the shell needs no early exit. A path without a short form falls back to PowerShell's call operator.
fn codex_command(executable: &Path, notify_path: &Path, quote: impl Fn(&str) -> String) -> String {
    if let (Some(executable), Some(notify)) = (cmd_word(executable), cmd_word(notify_path)) {
        return format!("{executable} agent-hook-notify-native {notify} codex");
    }
    format!(
        "& {} agent-hook-notify-native {} codex",
        quote(&executable.to_string_lossy()),
        quote(&notify_path.to_string_lossy())
    )
}

/// CDXC:AgentHooks 2026-10-16 WHY:
/// Hermes Agent (0.22.2) runs a hook without any shell: `split_command_line` splits it with Python's `shlex.split(posix=False)` and strips one layer of matching quotes per word, then `Popen(argv, shell=False)` starts it. The `sh -c '…powershell…'` command written before split into broken words, bash exited 2, and Hermes treats exit 2 from `pre_tool_call` as "block", so every tool call failed (GitHub #242). The command is gxserver and the notify script as plain words, double-quoted only when a path holds whitespace (no Windows path can hold a double quote).
fn hermes_command(executable: &Path, notify_path: &Path) -> String {
    let word = |path: &Path| {
        let text = path.to_string_lossy();
        if text.chars().any(char::is_whitespace) {
            format!("\"{text}\"")
        } else {
            text.into_owned()
        }
    };
    format!(
        "{} agent-hook-notify-native {} hermes-agent",
        word(executable),
        word(notify_path)
    )
}

/// `path` as one command word that cmd.exe, pwsh and Windows PowerShell all run unquoted: its
/// space-free short path with backslashes.
fn cmd_word(path: &Path) -> Option<String> {
    bare_command_path(path).map(|short| short.replace('/', "\\"))
}

/// `path` as a command word that Git Bash and PowerShell both run as it stands: its 8.3 short
/// form with forward slashes, when that has no space, quote or other character either shell
/// would read. `None` when the volume keeps no short names and the long path needs quoting.
pub(crate) fn bare_command_path(path: &Path) -> Option<String> {
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 1024];
    // SAFETY: `wide` is NUL-terminated and `buffer` holds `buffer.len()` UTF-16 units.
    let length = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(
            wide.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    let short = std::ffi::OsString::from_wide(&buffer[..length])
        .into_string()
        .ok()?
        .replace('\\', "/");
    short
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '/' | '.' | '_' | '~' | '-'))
        .then_some(short)
}

pub(crate) fn notify(args: Vec<String>) -> anyhow::Result<()> {
    let agent = args.get(1).map(String::as_str).unwrap_or("codex");
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut input = String::new();
        let result = std::io::stdin()
            .take(1024 * 1024)
            .read_to_string(&mut input)
            .map(|_| input);
        let _ = sender.send(result);
    });
    let input = receiver.recv_timeout(Duration::from_millis(1000));
    // CDXC:AgentHooks 2026-10-07 WHY: Cursor CLI runs Windows hooks as `$OutputEncoding = [System.Text.Encoding]::UTF8; Get-Content <payload> -Raw | & { $input | <command> }`, and that encoding writes a UTF-8 BOM before the JSON in pwsh and Windows PowerShell alike; serde rejected it ("expected value at line 1 column 1"), so no Cursor hook ever reached gxserver and Cursor chats never found their transcript.
    let parsed = input
        .as_ref()
        .ok()
        .and_then(|result| result.as_deref().ok())
        .and_then(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok());
    let state = [
        "VSMUX_SESSION_STATE_FILE",
        "GHOSTEX_SESSION_STATE_FILE",
        "ghostex_SESSION_STATE_FILE",
    ]
    .into_iter()
    .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
    .unwrap_or_default();
    if std::env::var("GHOSTEX_INTERNAL_PROMPT_GENERATION").as_deref() == Ok("1")
        || std::env::var("GHOSTEX_INTERNAL_TITLE_GENERATION").as_deref() == Ok("1")
        || (state.is_empty()
            && agent != "empryo"
            && [
                "GHOSTEX_GLOBAL_SESSION_REF",
                "GHOSTEX_GXSERVER_BASE_URL",
                "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
            ]
            .iter()
            .any(|key| std::env::var(key).map_or(true, |value| value.is_empty())))
    {
        // Like the bash notify script, a hook outside Ghostex still gives its agent the canned response.
        print_canned_response(agent, parsed.as_ref().unwrap_or(&Value::Null));
        return Ok(());
    }
    let input = input??;
    let mut payload = match parsed {
        Some(payload) => payload,
        None => serde_json::from_str(input.trim_start_matches('\u{feff}'))?,
    };
    if let Some(object) = payload.as_object_mut() {
        object.entry("agent").or_insert_with(|| json!(agent));
    }
    // The session's own Ghostex names its hook state folder; the script's folder is the fallback (CDXC:AgentHooks 2026-10-17 in server/src/zmx/env.rs).
    let directory = match std::env::var("GHOSTEX_AGENT_HOOK_STATE_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        Some(directory) => std::path::PathBuf::from(directory),
        None => {
            let script = std::fs::read_to_string(
                args.first()
                    .ok_or_else(|| anyhow::anyhow!("Missing hook path"))?,
            )?;
            super::install::notify_hook_state_directory(&script)
                .ok_or_else(|| anyhow::anyhow!("Missing hook state directory"))?
        }
    };
    let answer = super::run_notify_hook(vec![
        state,
        payload.to_string(),
        directory.to_string_lossy().into_owned(),
    ])
    .ok()
    .flatten();
    if let Some(answer) = answer {
        // A ZCode coordinator's SessionStart answer replaces the canned response.
        println!("{answer}");
        return Ok(());
    }
    print_canned_response(agent, &payload);
    Ok(())
}

fn print_canned_response(agent: &str, payload: &Value) {
    if agent != "antigravity" {
        if payload["hook_event_name"] == "Interrupt" {
            println!("{{}}");
        } else {
            println!("{{\"continue\":true}}");
        }
    }
}

/// Resolved over the live registry PATH, so a CLI installed while gxserver runs is not reported missing.
pub(crate) fn resolve_command(command: &str) -> Option<String> {
    crate::platform::live_path::find(command, &[]).map(|path| path.to_string_lossy().into_owned())
}
