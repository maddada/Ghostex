use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformShell {
    pub executable: String,
    kind: PlatformShellKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PlatformShellKind {
    Bash,
    Sh,
    Zsh,
    #[cfg(windows)]
    PowerShell,
}

/*
CDXC:PlatformSupport 2026-06-23-07:52:
server must run the same command, zmx, hook, and title-generation script contract on macOS and Ubuntu from one Rust codebase. Keep macOS pinned to /bin/zsh for exact existing behavior, while Linux executes the same POSIX script bodies through a deterministic installed shell instead of assuming zsh exists.
*/
pub fn command_shell() -> PlatformShell {
    #[cfg(target_os = "macos")]
    {
        PlatformShell::new("/bin/zsh")
    }

    #[cfg(windows)]
    {
        return PlatformShell::new(powershell_executable());
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        for candidate in command_shell_candidates() {
            if is_supported_shell(&candidate) && is_executable_file(Path::new(&candidate)) {
                return PlatformShell::new(candidate);
            }
        }
        PlatformShell::new("/bin/sh")
    }
}

pub fn command_shell_for_path(path: &str) -> PlatformShell {
    PlatformShell::new(path)
}

pub fn login_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(configured_shell) = env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        candidates.push(configured_shell);
    }
    candidates.extend(command_shell_candidates());
    dedupe(candidates)
}

/*
CDXC:ServerDaemon 2026-07-24:
Persistence-session interactive shells must be the user's own login shell, not
the pinned script shell: gxserver runs as a launchd agent whose environment
never saw the user's shell profiles, so pinning `/bin/zsh` gave bash/fish users
a shell that skipped their config (missing PATH entries, prompt tools like
oh-my-posh). Script bodies stay on `command_shell()` for POSIX compatibility;
only the final `exec` handed to the terminal user resolves through $SHELL and
the passwd entry.
*/
pub fn user_login_shell_exec_command() -> String {
    login_exec_command_for_shell_path(&user_login_shell_path())
}

pub(crate) fn user_login_shell_path() -> String {
    for candidate in user_login_shell_candidates() {
        if is_executable_file(Path::new(&candidate)) {
            return candidate;
        }
    }
    command_shell().executable
}

fn user_login_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(configured_shell) = env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        candidates.push(configured_shell);
    }
    if let Some(passwd_shell) = passwd_login_shell() {
        candidates.push(passwd_shell);
    }
    candidates.extend(command_shell_candidates());
    dedupe(candidates)
}

#[cfg(unix)]
fn passwd_login_shell() -> Option<String> {
    let shell = unsafe {
        let passwd = libc::getpwuid(libc::getuid());
        if passwd.is_null() {
            return None;
        }
        let raw_shell = (*passwd).pw_shell;
        if raw_shell.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr(raw_shell)
            .to_str()
            .ok()?
            .trim()
            .to_string()
    };
    (!shell.is_empty()).then_some(shell)
}

#[cfg(not(unix))]
fn passwd_login_shell() -> Option<String> {
    None
}

fn login_exec_command_for_shell_path(path: &str) -> String {
    match Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
    {
        "bash" | "zsh" => format!("exec {} -li", shell_quote(path)),
        "fish" | "nu" => format!("exec {} -l -i", shell_quote(path)),
        "sh" | "dash" | "ash" => format!("exec {} -i", shell_quote(path)),
        _ => format!("exec {}", shell_quote(path)),
    }
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The inside of a PowerShell single-quoted string for `text`: every character PowerShell reads
/// as a single quote is doubled.
///
/// CDXC:PlatformSupport 2026-10-25 WHY:
/// PowerShell ends a single-quoted string at U+0027 and also at the typographic quotes U+2018, U+2019, U+201A and U+201B. Doubling only the ASCII quote let a folder name, session title or prompt holding a curly quote close the literal, so the session startup failed to parse or ran the rest of the value as script (security audit 2026-10-25). Every PowerShell script Ghostex writes quotes values through this or `powershell_single_quote`.
/// SEE-ALSO: apps/desktop/src/app/helpers/remote/windows.rs, apps/mobile/app/src/remote/powershell.ts.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn powershell_single_quoted_content(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        if matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            escaped.push(ch);
        }
        escaped.push(ch);
    }
    escaped
}

/// `text` as one PowerShell single-quoted string literal.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn powershell_single_quote(text: &str) -> String {
    format!("'{}'", powershell_single_quoted_content(text))
}

/// The start, middle and end of the PowerShell expression [`powershell_native_arg`] writes for a
/// value holding a double quote, around its two single-quoted literals.
const NATIVE_ARG_START: &str = "$(if ($PSNativeCommandArgumentPassing -in 'Standard','Windows') { ";
const NATIVE_ARG_MIDDLE: &str = " } else { ";
const NATIVE_ARG_END: &str = " })";

/// `text` as one argument to a program (not a cmdlet) on a PowerShell command line, arriving whole
/// in Windows PowerShell 5.1 and in PowerShell 7.
///
/// CDXC:PlatformSupport 2026-11-13 WHY:
/// GitHub #582 (Q2785): on a PC with only Windows PowerShell 5.1 every Claude session Ghostex started quit with "Invalid JSON provided to --settings". PowerShell 5.1 (and 7 before 7.3, or with `$PSNativeCommandArgumentPassing = 'Legacy'`) builds a program's command line without escaping the double quotes inside an argument, so `'{"permissions":…}'` reached claude.exe as `{permissions:…}`; PowerShell 7.3 and later escape them. No one literal reads right in both (an argument escaped for 5.1 gains backslashes in 7), so a value holding a double quote becomes an expression that picks at run time: the value itself where PowerShell escapes for us, else the value already written as a Windows command-line argument: wrapped in double quotes, each inner quote doubled (`""`, which the C runtime, Rust and Zig all read as one quote inside quotes) and the backslashes before a quote doubled. Doubling keeps the quote count even before every space, so 5.1 neither wraps it again nor splits it at a space (`\"` did, checked on Windows PowerShell 5.1.26100 and PowerShell 7.6 on 2026-11-13). A value without a double quote stays a plain literal. Write a value this way only as a whole word: `-c=$(…)` is not expanded.
/// SEE-ALSO: server/src/agents/helpers.rs (`quote_shell_arg`), server/src/agents/home_secret_folders.rs (`read_quoted_word`, which reads it back).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn powershell_native_arg(text: &str) -> String {
    if !text.contains('"') {
        return powershell_single_quote(text);
    }
    format!(
        "{NATIVE_ARG_START}{}{NATIVE_ARG_MIDDLE}{}{NATIVE_ARG_END}",
        powershell_single_quote(text),
        powershell_single_quote(&windows_command_line_argument(text))
    )
}

/// `text` as one argument on a Windows command line, read back whole by the C runtime's rules.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_command_line_argument(text: &str) -> String {
    let mut argument = String::with_capacity(text.len() + 2);
    argument.push('"');
    let mut backslashes = 0;
    for ch in text.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                argument.push_str(&"\\".repeat(backslashes * 2));
                argument.push_str("\"\"");
                backslashes = 0;
            }
            _ => {
                argument.push_str(&"\\".repeat(backslashes));
                argument.push(ch);
                backslashes = 0;
            }
        }
    }
    argument.push_str(&"\\".repeat(backslashes * 2));
    argument.push('"');
    argument
}

/// The value of the argument [`powershell_native_arg`] wrote at the start of `text` (a plain
/// single-quoted literal or its expression), and the length it takes.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn read_powershell_native_arg(text: &str) -> Option<(String, usize)> {
    let Some(expression) = text.strip_prefix(NATIVE_ARG_START) else {
        return read_powershell_single_quoted(text);
    };
    let (value, value_len) = read_powershell_single_quoted(expression)?;
    let rest = expression[value_len..].strip_prefix(NATIVE_ARG_MIDDLE)?;
    let (_, legacy_len) = read_powershell_single_quoted(rest)?;
    rest[legacy_len..].strip_prefix(NATIVE_ARG_END)?;
    let len = text.len() - rest.len() + legacy_len + NATIVE_ARG_END.len();
    Some((value, len))
}

/// The value of the single-quoted literal at the start of `text` (as `powershell_single_quote`
/// writes it) and the length it takes.
#[cfg_attr(not(windows), allow(dead_code))]
fn read_powershell_single_quoted(text: &str) -> Option<(String, usize)> {
    let is_quote =
        |ch: char| matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}');
    let mut chars = text.char_indices().peekable();
    if !chars.next().is_some_and(|(_, ch)| is_quote(ch)) {
        return None;
    }
    let mut value = String::new();
    while let Some((index, ch)) = chars.next() {
        if !is_quote(ch) {
            value.push(ch);
        } else if let Some((_, quote)) = chars.next_if(|(_, next)| is_quote(*next)) {
            value.push(quote);
        } else {
            return Some((value, index + ch.len_utf8()));
        }
    }
    None
}

impl PlatformShell {
    fn new(path: impl Into<String>) -> Self {
        let executable = path.into();
        Self {
            kind: shell_kind_for_path(&executable),
            executable,
        }
    }

    pub fn script_args(&self, script: &str) -> Vec<String> {
        #[cfg(windows)]
        if matches!(self.kind, PlatformShellKind::PowerShell) {
            return powershell_script_args(script, false);
        }
        vec![self.command_flag(false).to_string(), script.to_string()]
    }

    pub fn interactive_script_args(&self, script: &str) -> Vec<String> {
        #[cfg(windows)]
        if matches!(self.kind, PlatformShellKind::PowerShell) {
            return powershell_script_args(script, false);
        }
        vec![self.command_flag(true).to_string(), script.to_string()]
    }

    /*
    CDXC:Zmx 2026-09-01:
    Probe and snapshot pipelines only invoke the bundled zmx binary, `ps`, and
    shell builtins; they run on every ~2s presentation poll and never present
    shell output to a user. Sourcing the login profile there costs tens of
    milliseconds per spawn and buys nothing, so those call sites use these
    profile-free args. Attach/run scripts and the launchd supervisor keep
    `command_flag`, because they hand the user their own shell.
    */
    pub fn profileless_script_args(&self, script: &str) -> Vec<String> {
        #[cfg(windows)]
        if matches!(self.kind, PlatformShellKind::PowerShell) {
            return powershell_script_args(script, true);
        }
        vec![
            self.profileless_command_flag().to_string(),
            script.to_string(),
        ]
    }

    pub fn profileless_command_flag(&self) -> &'static str {
        "-c"
    }

    pub fn command_flag(&self, interactive: bool) -> &'static str {
        match (&self.kind, interactive) {
            #[cfg(windows)]
            (PlatformShellKind::PowerShell, _) => "-Command",
            (PlatformShellKind::Bash | PlatformShellKind::Zsh, true) => "-lic",
            (PlatformShellKind::Bash | PlatformShellKind::Zsh, false) => "-lc",
            (PlatformShellKind::Sh, true) => "-ic",
            (PlatformShellKind::Sh, false) => "-c",
        }
    }

    pub fn command_string(&self, script: &str, interactive: bool) -> String {
        #[cfg(windows)]
        if matches!(self.kind, PlatformShellKind::PowerShell) {
            return format!(
                "& '{}' {}",
                powershell_single_quoted_content(&self.executable),
                powershell_script_args(script, !interactive).join(" ")
            );
        }
        format!(
            "{} {} {}",
            self.executable,
            self.command_flag(interactive),
            shell_quote(script)
        )
    }
}

fn command_shell_candidates() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        vec!["/bin/zsh".to_string()]
    }

    #[cfg(not(target_os = "macos"))]
    {
        vec![
            env::var("SHELL").unwrap_or_default(),
            "/bin/bash".to_string(),
            "/usr/bin/bash".to_string(),
            "/bin/zsh".to_string(),
            "/usr/bin/zsh".to_string(),
            "/bin/sh".to_string(),
            "/usr/bin/sh".to_string(),
        ]
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
    }
}

fn shell_kind_for_path(path: &str) -> PlatformShellKind {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    #[cfg(windows)]
    if ["pwsh.exe", "pwsh-preview.exe", "powershell.exe"]
        .iter()
        .any(|powershell| name.eq_ignore_ascii_case(powershell))
    {
        return PlatformShellKind::PowerShell;
    }
    match name {
        "bash" => PlatformShellKind::Bash,
        "zsh" => PlatformShellKind::Zsh,
        _ => PlatformShellKind::Sh,
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn is_supported_shell(path: &str) -> bool {
    matches!(
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default(),
        "bash" | "sh" | "zsh"
    )
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .filter(|value| seen.insert(PathBuf::from(value).to_string_lossy().to_string()))
        .collect()
}

/// PowerShell 7 wherever it is installed (see `ghostex_paths::powershell`), otherwise Windows
/// PowerShell 5.1. Looked up on every call, so a PowerShell 7 installed while gxserver runs is
/// used by the next session without a restart; sessions already running keep their shell.
#[cfg(windows)]
pub fn powershell_executable() -> String {
    ghostex_paths::powershell::preferred_powershell(
        &ghostex_paths::powershell::Environment::from_process(),
        crate::platform::live_path::directories,
    )
    .to_string_lossy()
    .into_owned()
}

#[cfg(windows)]
fn powershell_script_args(script: &str, profileless: bool) -> Vec<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let mut args = vec!["-NoLogo".into(), "-NonInteractive".into()];
    if profileless {
        args.push("-NoProfile".into());
    }
    args.push("-EncodedCommand".into());
    args.push(
        STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    args
}
